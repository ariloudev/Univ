//! A TAP query run as a job (IVOA UWS): submitted to the service's `/async` endpoint with
//! `PHASE=RUN`, its phase polled until it completes, then its result fetched. A service such as
//! the ESA Gaia archive answers a join of two large tables this way in seconds, where a
//! synchronous query of the same rows outlasts any request.

use std::time::Duration;

use reqwest::Url;

use super::{Attempt, Check, Clock, Fetcher, Item, judge_status, with_causes};
use crate::events::Event;

/// What a request's suffix starts with when the request is a job: the rest is its form.
pub const JOB: &str = "/async?";

/// How often a job's phase is asked for.
const POLL: Duration = Duration::from_secs(5);

/// How many times its phase is asked for before the job is given up on: half an hour.
const POLLS: u32 = 360;

/// How many polls in a row may fail before the job is given up on: a dropped connection is
/// asked again, not the job submitted again.
const MISSED: u32 = 3;

impl<C: Clock> Fetcher<C> {
    /// Runs the job whose form is `form` at the TAP service `base`, and returns its result's URL,
    /// body and rows.
    pub(super) async fn job(
        &self,
        item: &Item<'_>,
        base: &str,
        form: &str,
        check: &Check<'_>,
    ) -> Result<(String, Vec<u8>, u64), Attempt> {
        let submit = format!("{base}/async");
        self.sink.emit(Event::Get {
            source: item.source.clone(),
            item: item.name.to_owned(),
            url: submit.clone(),
        });
        let response = self
            .jobs
            .post(&submit)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form.to_owned())
            .send()
            .await
            .map_err(|e| Attempt::Retry(with_causes(&e)))?;
        if !response.status().is_redirection() {
            judge_status(response.status())?;
        }
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|l| l.to_str().ok())
            .ok_or_else(|| Attempt::Retry("the service gave no job to follow".to_owned()))?;
        let job = Url::parse(&submit)
            .and_then(|u| u.join(location))
            .map_err(|e| Attempt::GiveUp(format!("the job's address {location}: {e}")))?
            .to_string();
        self.until_done(&job).await?;
        let result = format!("{job}/results/result");
        let response = self
            .jobs
            .get(&result)
            .send()
            .await
            .map_err(|e| Attempt::Retry(with_causes(&e)))?;
        judge_status(response.status())?;
        let body = response
            .bytes()
            .await
            .map_err(|e| Attempt::Retry(with_causes(&e)))?;
        let rows = check(&body).map_err(Attempt::GiveUp)?;
        Ok((result, body.into(), rows))
    }

    /// Asks for the phase of `job` until it completes; an error when it ends otherwise or does
    /// not end in time.
    async fn until_done(&self, job: &str) -> Result<(), Attempt> {
        let mut missed = 0;
        for _ in 0..POLLS {
            let phase = match self.text(&format!("{job}/phase")).await {
                Ok(phase) => {
                    missed = 0;
                    phase
                }
                Err(Attempt::Retry(_)) if missed < MISSED => {
                    missed += 1;
                    self.clock.sleep(POLL).await;
                    continue;
                }
                Err(e) => return Err(e),
            };
            match phase.trim() {
                "COMPLETED" => return Ok(()),
                ended @ ("ERROR" | "ABORTED") => {
                    let why = self.text(&format!("{job}/error")).await.unwrap_or_default();
                    let why: String = why.chars().take(300).collect();
                    return Err(Attempt::Retry(format!(
                        "the job ended {ended}: {}",
                        why.trim()
                    )));
                }
                _ => self.clock.sleep(POLL).await,
            }
        }
        Err(Attempt::Retry(format!(
            "the job had not completed after {} minutes",
            POLL.as_secs() * u64::from(POLLS) / 60
        )))
    }

    async fn text(&self, url: &str) -> Result<String, Attempt> {
        let response = self
            .jobs
            .get(url)
            .send()
            .await
            .map_err(|e| Attempt::Retry(with_causes(&e)))?;
        judge_status(response.status())?;
        response
            .text()
            .await
            .map_err(|e| Attempt::Retry(with_causes(&e)))
    }
}
