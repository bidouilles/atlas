//! Dart in the code index through the language table (`src/langs.rs`): the
//! unit walk (`pubspec.yaml`), the scip_dart driver, the scanner when no
//! indexer runs, and the names it fills in where scip-dart keeps a
//! declaration local.
//!
//! No real scip_dart runs here: the driver is pointed at a stand-in script
//! that copies the checked-in `fixtures/scip/dart/index.scip` (written by
//! scip_dart 1.6.2 after `dart pub get --offline`, in a copy of the folder
//! under `/private/tmp`) to the output path it is given, or exits with
//! failure.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use atlas::code::{CodeSetup, Indexers, Unit};
use atlas::code_index::{self, IndexReport, RustSource};
use atlas::json;
use atlas::tools::{CodeState, Server};
use common::*;

fn dart_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join("scip").join("dart")
}

/// Copies the Dart fixture's sources (not its index) into `to`.
fn copy_sources(to: &Path) {
    fs::create_dir_all(to.join("lib")).unwrap();
    for f in ["pubspec.yaml", "lib/shapes.dart", "lib/scale.dart"] {
        fs::copy(dart_fixture().join(f), to.join(f)).unwrap();
    }
}

fn vaults(db: &Path) -> atlas::paths::Vaults {
    let d = db.parent().unwrap().join("notes");
    fs::create_dir_all(&d).unwrap();
    atlas::paths::Vaults::new(&[("notes".to_string(), d)]).unwrap()
}

fn setup(root: &Path, db: &Path, indexers: Indexers) -> CodeSetup {
    let spec = atlas::config::CodeSpec {
        roots: vec![atlas::config::CodeRootSpec {
            name: "app".into(),
            root: root.to_path_buf(),
            build_scripts: false,
            scip: Default::default(),
        }],
        debounce_s: Some(0),
        ..Default::default()
    };
    let mut code = CodeSetup::from_spec(&spec, db, false);
    code.defer_on_cargo = false;
    code.indexers = indexers;
    code
}

/// A stand-in indexer in `dir`: copies the fixture index to its second
/// argument (`--output FILE`), or exits 1 when `fail`.
fn stand_in(dir: &Path, fail: bool) -> PathBuf {
    fs::create_dir_all(dir).unwrap();
    fs::copy(dart_fixture().join("index.scip"), dir.join("index.scip")).unwrap();
    if cfg!(windows) {
        let p = dir.join("scip_dart.cmd");
        let body = if fail { "@exit /b 1\r\n".to_string() } else { "@copy /Y \"%~dp0index.scip\" %2 >NUL\r\n@exit /b 0\r\n".to_string() };
        fs::write(&p, body).unwrap();
        p
    } else {
        let p = dir.join("scip_dart");
        let body =
            if fail { "#!/bin/sh\nexit 1\n".to_string() } else { "#!/bin/sh\ncp \"$(dirname \"$0\")/index.scip\" \"$2\"\n".to_string() };
        fs::write(&p, body).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }
}

/// Indexes the root and opens a server over it.
fn indexed(root: &Path, db: &Path, indexers: Indexers) -> (IndexReport, Server) {
    let code = setup(root, db, indexers);
    drop(atlas::store::Store::open(db, vaults(db)).unwrap());
    let mut conn = rusqlite::Connection::open(db).unwrap();
    let rep = code_index::index_root(&code, &mut conn, &code.roots[0], &RustSource::ScanOnly("test".into())).unwrap();
    let store = atlas::store::Store::open(db, vaults(db)).unwrap();
    let mut s = Server::new(store);
    s.set_code(CodeState { setup: code, indexer: None });
    (rep, s)
}

/// `path:line kind container` for definitions, `path:line within` for
/// references.
fn answer(s: &mut Server, name: &str) -> (Vec<String>, Vec<String>) {
    let v = s.call("symbol", &json!({ "name": name })).unwrap();
    let list = |key: &str, f: &dyn Fn(&json::Value) -> String| -> Vec<String> { v[key].as_array().unwrap().iter().map(f).collect() };
    (
        list("defs", &|h| {
            format!("{}:{} {} {}", h["path"].as_str().unwrap(), h["line"], h["kind"].as_str().unwrap(), h["container"].as_str().unwrap())
        }),
        list("refs", &|h| format!("{}:{} {}", h["path"].as_str().unwrap(), h["line"], h["within"].as_str().unwrap())),
    )
}

#[test]
fn the_scanner_alone_defines_every_declaration() {
    let t = Temp::new("dart-scan");
    let root = t.path().join("app");
    copy_sources(&root);
    let db = t.path().join("state/atlas.sqlite");
    let (rep, mut s) = indexed(&root, &db, Indexers::Off("test".into()));
    assert_eq!(rep.units, [Unit { dir: String::new(), lang: "dart".into(), source: "scan only: test".into() }]);
    let (defs, refs) = answer(&mut s, "Square");
    assert_eq!(defs, ["lib/shapes.dart:13 class (scan) shapes", "lib/shapes.dart:14 ctor (scan) Square"]);
    assert!(refs.is_empty(), "the scanner gives definitions only");
    let (defs, _) = answer(&mut s, "fromJson");
    assert_eq!(defs, ["lib/shapes.dart:18 ctor (scan) Square"]);
    let (defs, _) = answer(&mut s, "_Unit");
    assert_eq!(defs, ["lib/shapes.dart:4 enum (scan) shapes"]);
    let conn = rusqlite::Connection::open(&db).unwrap();
    let r = atlas::recall::code_check(&conn, &setup(&root, &db, Indexers::Search).roots).unwrap();
    assert_eq!(r.langs.len(), 1);
    assert_eq!((r.langs[0].lang.as_str(), r.langs[0].missing), ("dart", 0), "{:?}", r.misses);
}

#[test]
fn scip_dart_rows_with_the_names_it_keeps_local_filled_in() {
    let t = Temp::new("dart-driver");
    let root = t.path().join("app");
    copy_sources(&root);
    let bin = stand_in(&t.path().join("bin"), false);
    let db = t.path().join("state/atlas.sqlite");
    let (rep, mut s) = indexed(&root, &db, Indexers::Fixed(vec![("scip_dart".into(), bin)]));
    assert_eq!(rep.units, [Unit { dir: String::new(), lang: "dart".into(), source: "scip-dart 1.6.2".into() }]);
    // SCIP rows, with references across files.
    let (defs, refs) = answer(&mut s, "Square");
    assert!(defs.iter().any(|d| d.starts_with("lib/shapes.dart:13 class ")), "{defs:?}");
    assert!(refs.iter().any(|r| r.starts_with("lib/scale.dart:3 ")), "{refs:?}");
    // scip-dart keeps a private enum local but defines its values on its
    // line: the scanner fills the enum in by name.
    let (defs, _) = answer(&mut s, "_Unit");
    assert_eq!(defs, ["lib/shapes.dart:4 enum (scan) shapes"]);
    let (defs, _) = answer(&mut s, "mm");
    assert_eq!(defs.len(), 1, "{defs:?}");
    // A type parameter (`Box<TRes>`) is neither a definition nor a
    // reference target, though scip-dart names it as a type of its file.
    let (defs, refs) = answer(&mut s, "TRes");
    assert!(defs.is_empty() && refs.is_empty(), "{defs:?} {refs:?}");
    let conn = rusqlite::Connection::open(&db).unwrap();
    let r = atlas::recall::code_check(&conn, &setup(&root, &db, Indexers::Search).roots).unwrap();
    assert_eq!((r.langs[0].lang.as_str(), r.langs[0].missing), ("dart", 0), "{:?}", r.misses);
}

#[test]
fn a_package_without_pub_get_is_scanned_and_says_why() {
    let t = Temp::new("dart-fail");
    let root = t.path().join("app");
    copy_sources(&root);
    let bin = stand_in(&t.path().join("bin"), true);
    let db = t.path().join("state/atlas.sqlite");
    let (rep, mut s) = indexed(&root, &db, Indexers::Fixed(vec![("scip_dart".into(), bin)]));
    assert_eq!(
        rep.units,
        [Unit {
            dir: String::new(),
            lang: "dart".into(),
            source: "scan only: scip_dart exited with Some(1); no .dart_tool: run dart pub get".into()
        }]
    );
    let (defs, _) = answer(&mut s, "total");
    assert_eq!(defs, ["lib/shapes.dart:34 property (scan) ShapeList", "lib/shapes.dart:39 fn (scan) shapes"]);
}
