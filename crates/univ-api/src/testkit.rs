//! What the native tests answer requests from: the release database as an in-memory SQLite
//! database with the real schema, and a way to run a handler's future to its end.

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use rusqlite::types::ValueRef;
use serde_json::Value;

use crate::backend::{Backend, BackendError, Row};
use crate::http::{Method, Request, Response};

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
    kept: crate::Kept,
    /// The univ-layers database, with its migrations applied.
    pub layers: rusqlite::Connection,
    /// The requests each (limiter, client) made; a limiter's limit is its own unless
    /// `limits` lowers it.
    pub counted: std::cell::RefCell<std::collections::HashMap<(crate::Limiter, String), u32>>,
    pub limits: std::cell::RefCell<std::collections::HashMap<crate::Limiter, u32>>,
    /// The owner secret.
    pub owner: Option<String>,
    /// The time, in milliseconds since the epoch: 2026-11-03T12:00:00Z unless set.
    pub now: std::cell::Cell<u64>,
    random: std::cell::Cell<u8>,
    /// The releases bucket, by key.
    pub bucket: std::cell::RefCell<std::collections::HashMap<String, Vec<u8>>>,
    /// How many statements were run, to show what the vocabulary keeping saves.
    pub statements: std::cell::Cell<usize>,
}

impl Sqlite {
    /// An empty release database.
    pub fn new() -> Self {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .execute_batch(univ_model::release::SCHEMA)
            .unwrap();
        Self {
            connection,
            kept: crate::Kept::default(),
            layers: layers_database(),
            counted: std::cell::RefCell::default(),
            limits: std::cell::RefCell::default(),
            owner: Some(OWNER_SECRET.to_owned()),
            now: std::cell::Cell::new(1_793_707_200_000),
            random: std::cell::Cell::new(0),
            bucket: std::cell::RefCell::default(),
            statements: std::cell::Cell::new(0),
        }
    }

    /// The release database, to read what a fixture needs.
    pub fn connection(&self) -> &rusqlite::Connection {
        &self.connection
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

    /// The answer to `request` sent with the owner secret, and its JSON document.
    pub fn as_owner(&self, request: Request) -> (Response, Value) {
        let response = self.answer(&request.with_bearer(OWNER_SECRET));
        let document = body(&response);
        (response, document)
    }

    /// Issues a key as `json` asks, as the owner, and gives the issued key with its secret.
    pub fn issue(&self, json: &Value) -> Value {
        let (response, issued) =
            self.as_owner(Request::new(Method::Post, "/v1/owner/keys").with_json(json));
        assert_eq!(response.status, 201, "{issued}");
        issued
    }

    /// The secret of a new key of the standard tier, issued by the owner.
    pub fn key(&self) -> String {
        self.issue(&serde_json::json!({"label": "test"}))["secret"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// The answer to `request` sent with key `secret`, and its JSON document, if any.
    pub fn with_key(&self, secret: &str, request: Request) -> (Response, Value) {
        let response = self.answer(&request.with_bearer(secret));
        let document = serde_json::from_slice(response.body.as_bytes()).unwrap_or(Value::Null);
        (response, document)
    }

    /// The answer to a GET of `path` with the raw query `query`.
    pub fn get(&self, path: &str, query: &str) -> Response {
        let mut request = Request::new(Method::Get, path);
        query.clone_into(&mut request.query);
        self.answer(&request)
    }
}

impl Sqlite {
    /// Puts `bytes` in the bucket at `key`.
    pub fn put(&self, key: &str, bytes: impl Into<Vec<u8>>) -> &Self {
        self.bucket
            .borrow_mut()
            .insert(key.to_owned(), bytes.into());
        self
    }

    /// The bytes a response carries, an object of the bucket read as the runtime would.
    pub fn bytes_of(&self, response: &Response) -> Vec<u8> {
        match &response.body {
            crate::Body::Object(key) => self.bucket.borrow()[key].clone(),
            body => body.as_bytes().to_vec(),
        }
    }

    /// The answer to a GET of `path` with the raw query `query`, and the JSON it carries.
    pub fn get_json(&self, path: &str, query: &str) -> (Response, Value) {
        let response = self.get(path, query);
        let document = body(&response);
        (response, document)
    }
}

/// The JSON document a response carries.
/// The JSON pointers a refusal names, sorted.
pub fn pointers(problem: &Value) -> Vec<String> {
    let mut list: Vec<String> = problem["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["pointer"].as_str().unwrap().to_owned())
        .collect();
    list.sort();
    list
}

pub fn body(response: &Response) -> Value {
    serde_json::from_slice(response.body.as_bytes())
        .unwrap_or_else(|error| panic!("{} carries no JSON: {error}", response.status))
}

impl Backend for Sqlite {
    fn releases(
        &self,
        sql: &str,
        params: &[Value],
    ) -> impl Future<Output = Result<Vec<Row>, BackendError>> {
        self.statements.set(self.statements.get() + 1);
        std::future::ready(run(&self.connection, sql, params))
    }

    fn releases_batch(
        &self,
        statements: &[(&str, &[Value])],
    ) -> impl Future<Output = Result<Vec<Vec<Row>>, BackendError>> {
        self.statements.set(self.statements.get() + 1);
        std::future::ready(
            statements
                .iter()
                .map(|(sql, params)| run(&self.connection, sql, params))
                .collect(),
        )
    }

    fn blob(&self, key: &str) -> impl Future<Output = Result<Option<Vec<u8>>, BackendError>> {
        std::future::ready(Ok(self.bucket.borrow().get(key).cloned()))
    }

    fn kept(&self) -> &crate::Kept {
        &self.kept
    }

    fn layers(
        &self,
        sql: &str,
        params: &[Value],
    ) -> impl Future<Output = Result<Vec<Row>, BackendError>> {
        std::future::ready(run(&self.layers, sql, params))
    }

    fn layers_batch(
        &self,
        statements: &[(&str, &[Value])],
    ) -> impl Future<Output = Result<Vec<Vec<Row>>, BackendError>> {
        std::future::ready(batch(&self.layers, statements))
    }

    fn random(&self, bytes: &mut [u8]) -> Result<(), BackendError> {
        // Deterministic, so tests can name what they expect; never random.
        for byte in bytes {
            let next = self.random.get().wrapping_add(1);
            self.random.set(next);
            *byte = next;
        }
        Ok(())
    }

    fn now_millis(&self) -> u64 {
        self.now.get()
    }

    fn limit(
        &self,
        limiter: crate::Limiter,
        client: &str,
    ) -> impl Future<Output = Result<bool, BackendError>> {
        let limit = self
            .limits
            .borrow()
            .get(&limiter)
            .copied()
            .unwrap_or_else(|| limiter.requests());
        let mut counted = self.counted.borrow_mut();
        let count = counted.entry((limiter, client.to_owned())).or_insert(0);
        *count += 1;
        std::future::ready(Ok(*count <= limit))
    }

    fn owner_secret(&self) -> Option<String> {
        self.owner.clone()
    }
}

/// The owner secret of the test kit's backend.
pub const OWNER_SECRET: &str = "the-owner-secret";

/// The univ-layers database in memory, with every migration of `migrations/` applied.
fn layers_database() -> rusqlite::Connection {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    // As D1 does, foreign keys are enforced.
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .unwrap();
    for migration in crate::MIGRATIONS {
        connection.execute_batch(migration).unwrap();
    }
    connection
}

/// Runs `statements` on `connection` in one transaction, as D1's batch does.
fn batch(
    connection: &rusqlite::Connection,
    statements: &[(&str, &[Value])],
) -> Result<Vec<Vec<Row>>, BackendError> {
    let error = |error: rusqlite::Error| BackendError(error.to_string());
    let transaction = connection.unchecked_transaction().map_err(error)?;
    let rows = statements
        .iter()
        .map(|(sql, params)| run(&transaction, sql, params))
        .collect::<Result<Vec<_>, _>>()?;
    transaction.commit().map_err(error)?;
    Ok(rows)
}

/// Runs one statement on `connection`, as D1 would, and gives every row.
fn run(
    connection: &rusqlite::Connection,
    sql: &str,
    params: &[Value],
) -> Result<Vec<Row>, BackendError> {
    let error = |error: rusqlite::Error| BackendError(error.to_string());
    let mut statement = connection.prepare(sql).map_err(error)?;
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
/// schema v3 details as Univ Ingest wrote them for its fixture release (abridged).
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
  '{"frame":"icrs","unit":"megaparsec","geometry":{"kind":"primitive","primitive":{"form":"sphere","centre":[-115.49361697310769,-107.49560811018048,162.22197055273585],"radius":46.55631096659769}}}',
  '{"from":["/geometry/Primitive/centre/lon","/geometry/Primitive/centre/lat","/geometry/Primitive/centre/radial/velocity","/geometry/Primitive/size/radius"],"frame_chain":[{"step":"to_icrs","from":"fk4_b1950","citation":"Standish 1982 (A&A 115, 20) and Aoki et al. 1983 (A&A 128, 263) as ERFA fk45z, zero FK5 proper motion; then Mignard & Froeschlé 2000 (A&A 354, 732)"}],"distance":{"from":"the velocity cz as published (its frame not stated)","method":"comoving distance of the redshift z = cz / c, taken as published and not moved, under the release cosmology"},"cosmology":"release"}',
  NULL,
  '[{"value":"/geometry/Primitive/size/radius","native":{"value":63.0,"unit":"megaparsec","hubble_constant":50.0,"assumed":{"cite":{"locator":{"form":"sentence","page":501,"quote":"The largest empty sphere which we can put into our sample has a radius of 63 Mpc (assuming H0 = 50 km s-1 Mpc-1) and is centered at α = 14h50m, δ = +46°, cz = 15,500 km s-1."},"paper":"kirshner-1987"},"cosmology":{"hubble_constant":50.0,"omega_lambda":null,"omega_matter":null},"stated":"H0 = 50 km/s/Mpc"}},"rescaled":{"value":46.55631096659769,"unit":"megaparsec"},"factor":0.7389890629618682,"method":"hubble_ratio","redshift":null}]');
INSERT INTO structure_details VALUES (2, 'local-void',
  '{"frame":"icrs","unit":"megaparsec","geometry":{"kind":"point","at":[23.975126304488985,-13.46203028720778,23.14088191179682]}}',
  '{"from":["/position/x","/position/y","/position/z"],"frame_chain":[{"step":"to_icrs","from":"supergalactic","citation":"Lahav et al. 2000, MNRAS 312, 166"}],"distance":{"from":"the published Cartesian velocities","method":"comoving distance of the redshift v / c of their magnitude, taken as published, under the release cosmology, along their direction"},"cosmology":"release"}',
  '{"grid":"cf4pp-density","field":"density","threshold":{"chosen":{"chosen":-1.0,"why":"A choice of this pipeline, not a published level: the earlier Univ void outlines drew the CF4++ density at -1. Tully et al. 2019 draw their Cosmicflows-3 Wiener-filter density at the under dense levels -0.7 and -1.1, and CF4++ (Courtois et al. 2025) is another reconstruction, the mean of HMC realizations of a linear density contrast, so neither level carries over by citation."}},"test":{"catalogues":["twomrs","sixdfgs-dr3","sdss-dr17-galaxies"],"dense_places":{"chosen_by":"pipeline","sources":["erass1-clusters"],"structure_kinds":["supercluster","supercluster_complex"],"why":"A choice of this pipeline, as in the earlier Univ void outlines, which refused a region that took in a named cluster or group: a region below the threshold that holds a matched object of the eRASS1 cluster catalogue (placed at its CMB-frame redshift in velocity space, as the grid is) or the published position of a researched supercluster or supercluster complex is not taken for the void. eRASS1 covers only the western Galactic hemisphere, so an outline in the eastern one is checked against the superclusters alone."},"min_expected":100.0,"ratio":0.85,"rule":"underdense_in_every_covering_catalogue","why":"Choices of this pipeline, as in the earlier Univ void outlines: a covering catalogue must find fewer than 0.85 of the galaxies that cells at the same distance outside the outline hold, and must expect at least 100 inside it to judge. Cosmicflows-4 is not among the catalogues: CF4++ is reconstructed from it, so it would test the grid against its own input."},"verdict":"refused","reason":"the outline was not drawn: its volume grid cf4pp-density is not among this release''s grids; the structure is shown at its published position","cells":0,"touches_edge":false,"checks":[],"dense":null}',
  '[{"value": "/values/long_dimension_of_inner_void", "native": {"value": 35.0, "unit": "megaparsec", "hubble_constant": 74.0, "assumed": {"cite": {"locator": {"form": "sentence", "page": null, "quote": "In Section 3.4, a weak preference will be found for H0=74 km s-1 Mpc-1."}, "paper": "tully-2008"}, "cosmology": {"hubble_constant": 74.0, "omega_lambda": null, "omega_matter": null}, "stated": "H0 = 74 km/s/Mpc, the paper''s preferred value for peculiar velocities; its sizes rest mostly on distances measured independently of redshift"}}, "rescaled": {"value": 38.27963346142477, "unit": "megaparsec"}, "factor": 1.093703813183565, "method": "hubble_ratio", "redshift": null}]');
INSERT INTO structure_details VALUES (2, 'giant-grb-ring',
  '{"frame":"icrs","unit":"megaparsec","geometry":{"kind":"members","members":[{"member":"tracer","at":[2446.1463137097526,1565.485715108069,849.6915183818139],"kind":"gamma-ray burst"},{"member":"tracer","at":[2118.9382314741592,51.15376954767522,2113.2303997257713],"kind":"gamma-ray burst"},{"member":"tracer","at":[2772.911962669102,457.03837698841323,1011.1885386336525],"kind":"gamma-ray burst"},{"member":"tracer","at":[2636.373267202268,623.1452426322156,1143.2166120971265],"kind":"gamma-ray burst"},{"member":"tracer","at":[1961.5575319021127,1664.9375575177044,1398.35655736851],"kind":"gamma-ray burst"},{"member":"tracer","at":[2714.4038300201933,-8.777330292579245,983.8894620697749],"kind":"gamma-ray burst"},{"member":"tracer","at":[2540.3673521426767,-332.48221428289423,1300.9090568074005],"kind":"gamma-ray burst"},{"member":"tracer","at":[1776.2991448361013,451.4308820708336,2200.67213650431],"kind":"gamma-ray burst"},{"member":"tracer","at":[1762.1825888406622,1328.8913904573421,1758.85267363113],"kind":"gamma-ray burst"}]}}',
  '{"from":["/geometry/Members"],"frame_chain":[{"step":"to_icrs","from":"galactic","citation":"ESA 1997, The Hipparcos Catalogue, vol. 1, §1.5.3"}],"distance":{"from":"each member''s matched object, or, for a member no object carries, its own sky position and redshift in the structure file","method":"a matched member at its object''s derived position; any other member a tracer at the comoving distance of its redshift, taken as heliocentric and moved to the CMB frame with the planck2018 dipole toward it, under the release cosmology"},"cosmology":"release"}',
  NULL,
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

/// Release 2's objects, names and designations, and an id ledger: 5 merged into 10 and 6 split
/// into 20 and 21 by release 2, 7 retired, 8 merged into 9 by release 1 and 9 into 30 by
/// release 2, and 50 and 40 merged into 10 only by release 3.
pub const OBJECTS: &str = "
INSERT INTO objects (release, id, kind, placed) VALUES
  (2, 10, 1, 1), (2, 11, 1, 1), (2, 12, 1, 1), (2, 20, 1, 1), (2, 21, 1, 1), (2, 30, 1, 1),
  (2, 50, 1, 1), (1, 5, 1, 1), (1, 6, 1, 1);
INSERT INTO names VALUES
  (2, 'm31', 10, 0, 'M 31'), (2, 'm31', 10, 0, 'M31'), (2, 'andromeda', 10, 0, 'Andromeda'),
  (2, 'twins', 11, 0, 'Twins'), (2, 'twins', 12, 0, 'Twins');
INSERT INTO designations VALUES (2, 0, 'HIP 1', 10), (2, 0, 'HIP 2', 11), (2, 0, 'HIP 2', 12);
INSERT INTO ids_replaced VALUES
  (5, 2, 'merged_into', '[10]'), (6, 2, 'split_into', '[20,21]'), (7, 2, 'retired', '[]'),
  (8, 1, 'merged_into', '[9]'), (9, 2, 'merged_into', '[30]'),
  (50, 3, 'merged_into', '[10]'), (40, 3, 'merged_into', '[10]');
";

/// Release 2 of Univ Ingest's fixture, with every object and the vocabulary it is coded in.
pub const FIXTURE_OBJECTS: &str = include_str!("testkit_objects.sql");

/// Release 2's volume grid, with the scale domains of [`STRUCTURES`].
pub const GRIDS: &str = r#"
INSERT INTO grids VALUES (2, 'cf4-basins', 1, 'megaparsec_over_h', '[128,128,128]',
  '[{"field":"basin_label","kind":"reconstructed","unit":"dimensionless"},{"field":"basin_label","kind":"reconstructed","unit":"dimensionless"}]',
  'releases/2/grids/cf4-basins/header.json');
"#;

/// The cosmic web tile manifest of release 2: two nodes.
pub const COSMIC_WEB_MANIFEST: &str = r#"{"format":"univ-tile-manifest","release":2,
  "scale_domain":"cosmic_web","encoding":{"version":1},"nodes":[
  {"address":"r","level":0,"bounds":{"min":[-168.0,-168.0,-168.0],"max":[168.0,168.0,168.0]},"bytes":3,"sha256":"aaaa"},
  {"address":"r07","level":2,"bounds":{"min":[-84.0,-84.0,-84.0],"max":[0.0,0.0,0.0]},"bytes":2,"sha256":"bbbb"}]}"#;
