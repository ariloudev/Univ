//! What the native tests answer requests from: the release database as an in-memory SQLite
//! database with the real schema, and a way to run a handler's future to its end.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use rusqlite::types::ValueRef;
use serde_json::Value;

use crate::backend::{Backend, BackendError, Row};
use crate::http::{Request, Response};

/// Runs `future` to its end. The backends of the tests never wait, so a future that is not
/// ready after many polls is a fault of the test.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..1_000 {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
    panic!("the future never finished");
}

/// The release database, with univ-model's schema, in memory.
pub struct Sqlite {
    connection: rusqlite::Connection,
}

impl Sqlite {
    /// An empty release database.
    pub fn new() -> Self {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(univ_model::release::SCHEMA)
            .unwrap();
        Self { connection }
    }

    /// Runs `sql`, statements that write the fixture's rows.
    pub fn exec(&self, sql: &str) -> &Self {
        self.connection.execute_batch(sql).unwrap();
        self
    }

    /// The answer to `request`.
    pub fn answer(&self, request: &Request) -> Response {
        block_on(crate::handle(request, self))
    }
}

impl Backend for Sqlite {
    fn releases(
        &self,
        sql: &str,
        params: &[Value],
    ) -> impl Future<Output = Result<Vec<Row>, BackendError>> {
        std::future::ready(self.query(sql, params))
    }
}

impl Sqlite {
    /// Runs one statement, as D1 would, and gives every row.
    fn query(&self, sql: &str, params: &[Value]) -> Result<Vec<Row>, BackendError> {
        let error = |error: rusqlite::Error| BackendError(error.to_string());
        let mut statement = self.connection.prepare(sql).map_err(error)?;
        let names: Vec<String> = statement
            .column_names()
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let params: Vec<rusqlite::types::Value> = params.iter().map(sql_value).collect();
        let mut rows = statement
            .query(rusqlite::params_from_iter(params))
            .map_err(error)?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(error)? {
            let mut fields = Row::new();
            for (i, name) in names.iter().enumerate() {
                fields.insert(name.clone(), json_value(row.get_ref(i).map_err(error)?));
            }
            out.push(fields);
        }
        Ok(out)
    }
}

/// A bound parameter as SQLite takes it, the way D1 binds JSON values.
fn sql_value(value: &Value) -> rusqlite::types::Value {
    match value {
        Value::Null => rusqlite::types::Value::Null,
        Value::Bool(flag) => rusqlite::types::Value::Integer(i64::from(*flag)),
        Value::Number(number) => number.as_i64().map_or_else(
            || rusqlite::types::Value::Real(number.as_f64().unwrap()),
            rusqlite::types::Value::Integer,
        ),
        Value::String(text) => rusqlite::types::Value::Text(text.clone()),
        other => rusqlite::types::Value::Text(other.to_string()),
    }
}

/// A column's value as D1 returns it in JSON.
fn json_value(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(integer) => Value::from(integer),
        ValueRef::Real(real) => Value::from(real),
        ValueRef::Text(text) => Value::from(String::from_utf8_lossy(text).into_owned()),
        ValueRef::Blob(_) => panic!("the release database holds no blobs"),
    }
}

/// Three releases: 1 published and pruned, 2 published and latest, 3 built but never
/// published, each with two sources.
pub const RELEASES: &str = r#"
INSERT INTO releases VALUES (1, '2026-11-01T10:00:00Z', '2026-11-01T11:00:00Z',
  '2026-11-02T09:00:00Z',
  '{"name":"Planck 2018","hubble_constant":67.66,"omega_matter":0.3111}', 300.0, 'm', 'i', 2,
  'rules', 'rule', 'name', '["hipparcos-2007","twomrs"]', 10, 2, 0, 3, 1000);
INSERT INTO releases VALUES (2, '2026-11-02T08:00:00Z', '2026-11-02T08:30:00Z', NULL,
  '{"name":"Planck 2018","hubble_constant":67.66,"omega_matter":0.3111}', 300.0, 'm', 'i', 2,
  'rules', 'rule', 'name', '["hipparcos-2007","twomrs"]', 12, 2, 1, 4, 1200);
INSERT INTO releases VALUES (3, '2026-11-03T08:00:00Z', NULL, NULL,
  '{"name":"Planck 2018","hubble_constant":67.66,"omega_matter":0.3111}', 300.0, 'm', 'i', 2,
  'rules', 'rule', 'name', '["hipparcos-2007"]', 13, 2, 1, 4, 1300);
INSERT INTO latest VALUES (1, 2, '2026-11-02T08:30:00Z');
"#;

/// Release 2's two sources: Hipparcos, fetched in two payloads with a fix and rows left out,
/// and the researched structures, which have no primary reference and nothing fetched.
pub const SOURCES: &str = r#"
INSERT INTO sources VALUES (2, 0, 'hipparcos-2007', 'Hipparcos, the new reduction', 'vizier',
  'stars', 'van Leeuwen F. 2007, A&A 474, 653. 2007A&A...474..653V, doi:10.1051/0004-6361:20078357.',
  'https://cds.unistra.fr/vizier-org/licences_vizier.html', 'The data are free of usage in a scientific context.',
  'This research has made use of VizieR.', 'permitted_with_credit', 'provisional', 117955,
  '{"method":"vizier_tsv","table":"I/311/hip2","constraints":[],"slices":{"column":"HIP","from":1,"to":120404,"step":30000},"rows":117955}',
  '["No server-side cut: every row of I/311/hip2."]',
  '[{"defect":"RArad and DErad are served in degrees.","change":"RArad and DErad are read as degrees."}]');
INSERT INTO sources VALUES (2, 1, 'researched-structures', 'Researched structures', 'literature',
  'cosmic_web', 'Each structure cites its own papers.', 'https://example.org/terms', 'Quoted.',
  'Credit each paper.', 'permitted_with_credit', 'reviewed', 0, '{"method":"files"}', '[]', '[]');
INSERT INTO payloads VALUES (2, 0, 'chunk-0', 'https://vizier.cds.unistra.fr/viz-bin/asu-tsv?-source=I/311/hip2&HIP=1..30000',
  'aa11', 2048, '2026-11-01T09:00:00Z', 29999);
INSERT INTO payloads VALUES (2, 0, 'chunk-1', 'https://vizier.cds.unistra.fr/viz-bin/asu-tsv?-source=I/311/hip2&HIP=30001..60000',
  'bb22', 4096, '2026-11-01T09:01:00Z', NULL);
INSERT INTO normalizations VALUES (2, 0, 117950, '{"no_parallax":3,"bad_position":2}',
  '[{"change":"RArad and DErad are read as degrees.","entries_changed":117955}]');
INSERT INTO source_details VALUES (2, 0, '2007A&A...474..653V', '10.1051/0004-6361:20078357', NULL,
  '{"order":5,"cells":[1,2,3]}',
  '[{"measure":"parallax","min":0.1,"max":772.33,"unit":"milliarcsecond","basis":"parallax as published","entries":117950}]');
INSERT INTO source_details VALUES (2, 1, NULL, NULL, 'A compilation: each structure cites its own papers.', NULL, NULL);
"#;

/// Release 2's scale domains and three structures: a void published under H0 = 50 and placed
/// as a sphere, a void outlined from a reconstruction, and a ring of tracers; with their
/// schema v3 details.
pub const STRUCTURES: &str = r#"
INSERT INTO scale_domains VALUES (2, 'cosmic_web', 'Mpc', 168.0, 6, 1, 'releases/2/tiles/cosmic_web/manifest.json');
INSERT INTO scale_domains VALUES (2, 'stars', 'pc', 48.0, 1, 1, 'releases/2/tiles/stars/manifest.json');
INSERT INTO structures VALUES (2, 'bootes-void', 'Bootes Void', 'void', 'observed', 'cosmic_web',
  -115.49, -107.35, 162.22, 'primitive',
  '{"papers":[{"id":"kirshner-1987","bibcode":"1987ApJ...314..493K","doi":"10.1086/165080"}],"values":{"radius":{"quantity":{"value":63.0,"unit":"megaparsec","uncertainty":{"form":"none_reported"}},"kind":"inferred","cite":{"paper":"kirshner-1987","locator":{"form":"sentence","quote":"a radius of 63 Mpc (assuming H0 = 50 km s-1 Mpc-1)","page":501}},"assumes":{"cosmology":{"hubble_constant":50.0}}}}}',
  NULL, '{"min":[-162.0,-154.0,115.7],"max":[-68.9,-60.9,208.8]}');
INSERT INTO structures VALUES (2, 'local-void', 'Local Void', 'void', 'reconstructed', 'cosmic_web',
  23.98, -13.46, 23.14, 'point',
  '{"values":{"depth":{"quantity":{"value":-1.0,"unit":"dimensionless","uncertainty":{"form":"none_reported"}},"kind":"reconstructed","cite":{"paper":"tully-2019","locator":{"form":"figure","figure":"3"}}},"edge":{"quantity":{"value":20.0,"unit":"megaparsec","uncertainty":{"form":"none_reported"}},"kind":"estimated","cite":{"paper":"tully-2019","locator":{"form":"figure","figure":"5"}}}},"outline":{"grid":"cf4pp-density","field":"density","threshold":{"chosen":{"chosen":-1.0}},"side":"below","test":{"rule":"underdense_in_every_covering_catalogue"}}}',
  NULL, '{"min":[23.98,-13.46,23.14],"max":[23.98,-13.46,23.14]}');
INSERT INTO structures VALUES (2, 'giant-grb-ring', 'Giant GRB Ring', 'ring', 'statistical_claim', 'cosmic_web',
  NULL, NULL, NULL, 'members', '{"values":{}}',
  '{"shape":{"members":[{"member":"tracer","kind":"gamma-ray burst","at":[1.0,2.0,3.0]},{"member":"object","id":42,"at":[4.0,5.0,6.0]}]}}',
  NULL);
INSERT INTO structure_details VALUES (2, 'bootes-void',
  '{"kind":"primitive","frame":"icrs","unit":"Mpc","shape":"sphere","centre":[-115.49,-107.35,162.22],"radius":46.56}',
  '{"from":["/position/lon","/position/lat","/position/radial/velocity","/values/radius"],"frame_chain":["fk4_b1950 -> icrs"],"distance":{"from":"/position/radial/velocity","method":"comoving distance from the CMB-frame redshift under the release cosmology"},"cosmology":"release"}',
  NULL,
  '[{"value":"/values/radius","native":{"value":63.0,"unit":"megaparsec","hubble_constant":50.0,"assumed":{"cosmology":{"hubble_constant":50.0}}},"rescaled":{"value":46.56,"unit":"megaparsec"},"factor":0.739,"method":"hubble_ratio"}]');
INSERT INTO structure_details VALUES (2, 'local-void', NULL, NULL,
  '{"grid":"cf4pp-density","field":"density","threshold":-1.0,"test":{"rule":"underdense_in_every_covering_catalogue"},"verdict":"refused","reason":"the region touches the grid edge","cells":0,"touches_edge":true,"checks":[]}',
  NULL);
"#;

/// 1,200 more structures of release 2, `filler-0000` to `filler-1199`, alternating between
/// walls and filaments, for paging.
pub const MANY_STRUCTURES: &str = "
WITH RECURSIVE n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n WHERE i < 1199)
INSERT INTO structures
SELECT 2, printf('filler-%04d', i), printf('Filler %d', i),
  CASE i % 2 WHEN 0 THEN 'wall' ELSE 'filament' END, 'observed', 'cosmic_web',
  i, 0, 0, 'point', '{}', NULL, NULL FROM n;
";
