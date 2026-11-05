//! A TAP query run as a job (IVOA UWS): submitted to the service's `/async` endpoint with
//! `PHASE=RUN`, its phase polled until it completes, then its result fetched and the job deleted.
//! A service such as the ESA Gaia archive answers a join of two large tables this way in
//! seconds, where a synchronous query of the same rows outlasts any request.

use std::future::Future;
use std::time::Duration;

use reqwest::header::CONTENT_TYPE;

use super::{Attempt, Clock, Fetcher, read};

/// Where a TAP service takes jobs, under its base address.
pub(super) const JOBS: &str = "/async";

/// How long to wait before each poll of a job's phase.
const POLL: Duration = Duration::from_secs(5);

/// How long a job may run before it is given up on.
const PATIENCE: Duration = Duration::from_mins(30);

/// How many times in a row one of a job's requests may fail, as a dropped connection does,
/// before the job is given up on: the request is made again, not the job submitted again.
const MISSED: u32 = 3;

impl<C: Clock> Fetcher<C> {
    /// Runs the job whose form is `form` at the TAP service `base` and gives its result. A job
    /// that ends in error, or does not complete in time, sends the fetch to the next mirror:
    /// the same query submitted again would end the same way.
    pub(super) async fn job(&self, base: &str, form: &str) -> Result<Vec<u8>, Attempt> {
        let job = self.submit(base, form).await?;
        let result = format!("{job}/results/result");
        let result = match self.until_done(&job).await {
            Ok(()) => self.again(|| read(&self.jobs, &result)).await,
            Err(e) => Err(e),
        };
        // The service keeps a job and its result until it is deleted or expires; deleting one
        // still running also stops it. Whether the deletion is honoured changes nothing here.
        let _ = self.jobs.delete(&job).send().await;
        result
    }

    /// Submits the job and gives its address, where the service redirects the submission.
    async fn submit(&self, base: &str, form: &str) -> Result<String, Attempt> {
        let submit = format!("{base}{JOBS}");
        let response = self
            .jobs
            .post(&submit)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form.to_owned())
            .send()
            .await
            .map_err(|e| Attempt::Retry(super::with_causes(&e)))?;
        super::judge_status(response.status())?;
        let job = response.url().as_str().trim_end_matches('/');
        if job == submit {
            return Err(Attempt::GiveUp(
                "the service took the job without giving its address".to_owned(),
            ));
        }
        Ok(job.to_owned())
    }

    /// Polls the phase of `job` until it completes; an error when it ends otherwise or does not
    /// end in time.
    async fn until_done(&self, job: &str) -> Result<(), Attempt> {
        let phase = format!("{job}/phase");
        for _ in 0..PATIENCE.as_secs() / POLL.as_secs() {
            self.clock.sleep(POLL).await;
            let now = self.again(|| read(&self.jobs, &phase)).await?;
            match String::from_utf8_lossy(&now).trim() {
                "COMPLETED" => return Ok(()),
                ended @ ("ERROR" | "ABORTED") => {
                    let why = read(&self.jobs, &format!("{job}/error"))
                        .await
                        .unwrap_or_default();
                    let why: String = String::from_utf8_lossy(&why)
                        .trim()
                        .chars()
                        .take(300)
                        .collect();
                    return Err(Attempt::GiveUp(format!("the job ended {ended}: {why}")));
                }
                _ => {}
            }
        }
        Err(Attempt::GiveUp(format!(
            "the job had not completed after {} minutes",
            PATIENCE.as_secs() / 60
        )))
    }

    /// `request`, made again after a wait while it fails in a way worth retrying, up to
    /// [`MISSED`] times.
    async fn again<T, F, Fut>(&self, request: F) -> Result<T, Attempt>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, Attempt>>,
    {
        let mut missed = 0;
        loop {
            match request().await {
                Err(Attempt::Retry(_)) if missed < MISSED => {
                    missed += 1;
                    self.clock.sleep(POLL).await;
                }
                done => return done,
            }
        }
    }
}
