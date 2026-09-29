use std::fs;
use std::time::Duration;

use serde_json::json;

use super::fake::{FakeServer, Reply};
use crate::assets::AssetKind;
use crate::library::polyhaven::{Limits, PolyHaven};
use crate::library::{LibraryItem, LibraryProvider, LibraryQuery};

fn ph(base: &str) -> PolyHaven {
    PolyHaven { base: base.into() }
}

fn q(text: &str, kind: Option<AssetKind>, limit: Option<u32>) -> LibraryQuery {
    LibraryQuery {
        text: text.into(),
        kind,
        limit,
    }
}

fn item(id: &str, kind: AssetKind) -> LibraryItem {
    LibraryItem {
        provider: "polyhaven".into(),
        id: id.into(),
        title: id.into(),
        kind,
        license: Default::default(),
        thumbnail_url: None,
        page_url: None,
    }
}

fn models_json() -> String {
    json!({
        "wooden_crate": {"name": "Wooden Crate", "type": 2, "authors": {"Ana": "all", "Ben": "all"},
            "categories": ["props", "containers"], "tags": ["box", "wood"], "thumbnail_url": "https://cdn.example/crate.png"},
        "apple": {"name": "Apple", "type": 2, "authors": {"Ana": "all"}, "categories": ["food"], "tags": ["fruit"]},
    })
    .to_string()
}

fn textures_json() -> String {
    json!({
        "brick_wall": {"name": "Brick Wall", "type": 1, "authors": {"Cy": "all"}, "categories": ["brick"], "tags": ["wall", "wood-like"]},
    })
    .to_string()
}

fn listing_server() -> FakeServer {
    FakeServer::start(|url, _| match url {
        "/assets?t=models" => Reply::ok(models_json()),
        "/assets?t=textures" => Reply::ok(textures_json()),
        _ => Reply::status(404),
    })
}

#[test]
fn search_filters_sorts_and_licenses_items() {
    let server = listing_server();
    let items = ph(&server.base).search(&q("WOOD", None, None)).unwrap();
    // "Wooden Crate" by name/tag, "Brick Wall" by its "wood-like" tag.
    let titles: Vec<_> = items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, ["Brick Wall", "Wooden Crate"]);
    let crate_item = &items[1];
    assert_eq!(crate_item.kind, AssetKind::Model3d);
    assert_eq!(items[0].kind, AssetKind::Image);
    assert_eq!(crate_item.license.name.as_deref(), Some("CC0-1.0"));
    assert_eq!(crate_item.license.source.as_deref(), Some("Poly Haven"));
    assert_eq!(crate_item.license.author.as_deref(), Some("Ana, Ben"));
    assert_eq!(
        crate_item.license.url.as_deref(),
        Some("https://polyhaven.com/a/wooden_crate")
    );
    assert_eq!(crate_item.page_url, crate_item.license.url);
    assert_eq!(
        crate_item.thumbnail_url.as_deref(),
        Some("https://cdn.example/crate.png")
    );
    assert!(
        server
            .seen
            .lock()
            .unwrap()
            .iter()
            .all(|s| s.0 == "InfinaBox")
    );
}

#[test]
fn search_by_kind_only_asks_for_that_list() {
    let server = listing_server();
    let items = ph(&server.base)
        .search(&q("", Some(AssetKind::Model3d), None))
        .unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Apple");
    assert_eq!(server.paths(), ["/assets?t=models"]);
    let none = ph(&server.base)
        .search(&q("", Some(AssetKind::Audio), None))
        .unwrap();
    assert!(none.is_empty());
}

#[test]
fn search_caps_at_limit() {
    let mut map = serde_json::Map::new();
    for i in 0..80 {
        map.insert(
            format!("m{i:02}"),
            json!({"name": format!("Thing {i:02}"), "type": 2}),
        );
    }
    let body = serde_json::Value::Object(map).to_string();
    let server = FakeServer::start(move |_, _| Reply::ok(body.clone()));
    let p = ph(&server.base);
    let kind = Some(AssetKind::Model3d);
    assert_eq!(p.search(&q("", kind, None)).unwrap().len(), 24);
    assert_eq!(p.search(&q("", kind, Some(500))).unwrap().len(), 60);
    assert_eq!(p.search(&q("", kind, Some(5))).unwrap().len(), 5);
}

#[test]
fn wrong_status_is_a_plain_sentence() {
    let server = FakeServer::start(|_, _| Reply::status(500));
    let err = ph(&server.base).search(&q("x", None, None)).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Poly Haven didn't understand the request (status 500)."
    );
}

#[test]
fn unreachable_server_is_a_plain_sentence() {
    // Start then drop, so the port is closed.
    let base = {
        let s = FakeServer::start(|_, _| Reply::status(200));
        s.base.clone()
    };
    std::thread::sleep(Duration::from_millis(200));
    let err = ph(&base).search(&q("x", None, None)).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Couldn't reach Poly Haven. Check your internet connection."
    );
}

#[test]
fn bad_json_and_non_web_base_are_plain_sentences() {
    let server = FakeServer::start(|_, _| Reply::ok("not json"));
    let err = ph(&server.base).search(&q("", None, None)).unwrap_err();
    assert!(err.to_string().contains("couldn't read"), "{err}");
    let err = ph("file:///etc").search(&q("", None, None)).unwrap_err();
    assert!(err.to_string().contains("web address"), "{err}");
}

#[test]
fn huge_json_body_is_refused() {
    let server = FakeServer::start(|_, _| Reply::ok(vec![b' '; 5000]));
    let limits = Limits {
        json_cap: 1000,
        ..Limits::default()
    };
    let err = ph(&server.base)
        .search_with(&q("", None, None), limits)
        .unwrap_err();
    assert!(err.to_string().contains("more than expected"), "{err}");
}

#[test]
fn slow_server_times_out() {
    let server = FakeServer::start(|_, _| Reply::ok("{}").delayed(Duration::from_secs(3)));
    let limits = Limits {
        total_timeout: Duration::from_millis(500),
        ..Limits::default()
    };
    let err = ph(&server.base)
        .search_with(&q("", None, None), limits)
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Poly Haven took too long to answer. Try again in a moment."
    );
}

#[test]
fn redirect_to_a_different_host_is_refused() {
    let target = FakeServer::start(|_, _| Reply::ok("{}"));
    // Same machine, different host name: not the host the request went to.
    let other = target.base.replace("127.0.0.1", "localhost");
    let server = FakeServer::start(move |url, _| Reply::redirect(&format!("{other}{url}")));
    let err = ph(&server.base).search(&q("", None, None)).unwrap_err();
    assert!(err.to_string().contains("somewhere else"), "{err}");
    assert!(target.paths().is_empty(), "the other host was contacted");
}

#[test]
fn redirect_on_the_same_host_is_followed() {
    let server = FakeServer::start(|url, _| {
        if url.starts_with("/assets") {
            Reply::redirect("/moved")
        } else {
            Reply::ok(models_json())
        }
    });
    let items = ph(&server.base)
        .search(&q("apple", Some(AssetKind::Model3d), None))
        .unwrap();
    assert_eq!(items.len(), 1);
}

fn model_server() -> FakeServer {
    FakeServer::start(|url, base| match url {
        "/files/crate" => Reply::ok(
            json!({"gltf": {
                "2k": {"gltf": {"url": format!("{base}/dl/2k/crate_2k.gltf"), "size": 1}},
                "1k": {"gltf": {
                    "url": format!("{base}/dl/crate_1k.gltf"), "size": 12, "md5": "x",
                    "include": {
                        "crate.bin": {"url": format!("{base}/dl/crate.bin"), "size": 4},
                        "textures/crate_diff.jpg": {"url": format!("{base}/dl/tex.jpg"), "size": 3},
                    }}},
            }, "blend": {}})
            .to_string(),
        ),
        "/files/evil" => Reply::ok(
            json!({"gltf": {"1k": {"gltf": {"url": format!("{base}/dl/a.gltf"),
                "include": {"../escape.bin": {"url": format!("{base}/dl/crate.bin")}}}}}})
            .to_string(),
        ),
        "/files/short" => Reply::ok(
            json!({"gltf": {"1k": {"gltf": {"url": format!("{base}/dl/crate.bin"), "size": 999}}}})
                .to_string(),
        ),
        "/files/big" => Reply::ok(
            json!({"gltf": {"1k": {"gltf": {"url": format!("{base}/dl/huge.gltf")}}}}).to_string(),
        ),
        "/dl/crate_1k.gltf" => Reply::ok("{\"asset\":1}\n"),
        "/dl/crate.bin" => Reply::ok("BIN!"),
        "/dl/tex.jpg" => Reply::ok("JPG"),
        "/dl/huge.gltf" => Reply::ok(vec![0u8; 5000]),
        "/dl/a.gltf" => Reply::ok("a"),
        _ => Reply::status(404),
    })
}

#[test]
fn fetches_a_model_with_its_include_files_at_1k() {
    let server = model_server();
    let p = ph(&server.base);
    let dest = tempfile::tempdir().unwrap();
    let files = p
        .fetch(&item("crate", AssetKind::Model3d), dest.path())
        .unwrap();
    let root = dest.path().join("crate");
    assert_eq!(files[0], root.join("crate_1k.gltf"));
    assert_eq!(fs::read_to_string(&files[0]).unwrap(), "{\"asset\":1}\n");
    assert_eq!(fs::read(root.join("crate.bin")).unwrap(), b"BIN!");
    assert_eq!(
        fs::read(root.join("textures/crate_diff.jpg")).unwrap(),
        b"JPG"
    );
    assert_eq!(files.len(), 3);
    assert!(!server.paths().iter().any(|p| p.contains("2k")));
}

#[test]
fn fetch_never_overwrites_and_numbers_the_folder() {
    let server = model_server();
    let p = ph(&server.base);
    let dest = tempfile::tempdir().unwrap();
    let it = item("crate", AssetKind::Model3d);
    p.fetch(&it, dest.path()).unwrap();
    fs::write(dest.path().join("crate/crate.bin"), "MINE").unwrap();
    let again = p.fetch(&it, dest.path()).unwrap();
    assert!(again[0].starts_with(dest.path().join("crate-2")));
    assert_eq!(
        fs::read(dest.path().join("crate/crate.bin")).unwrap(),
        b"MINE"
    );
}

#[test]
fn fetch_refuses_unsafe_paths_and_ids() {
    let server = model_server();
    let p = ph(&server.base);
    let dest = tempfile::tempdir().unwrap();
    let err = p
        .fetch(&item("evil", AssetKind::Model3d), dest.path())
        .unwrap_err();
    assert!(err.to_string().contains("file name"), "{err}");
    assert_eq!(fs::read_dir(dest.path()).unwrap().count(), 0);
    let err = p
        .fetch(&item("../x", AssetKind::Model3d), dest.path())
        .unwrap_err();
    assert!(err.to_string().contains("isn't a Poly Haven item"), "{err}");
}

#[test]
fn fetch_cleans_up_when_a_download_is_wrong() {
    let server = model_server();
    let p = ph(&server.base);
    let dest = tempfile::tempdir().unwrap();
    let err = p
        .fetch(&item("short", AssetKind::Model3d), dest.path())
        .unwrap_err();
    assert!(err.to_string().contains("incomplete"), "{err}");
    assert_eq!(fs::read_dir(dest.path()).unwrap().count(), 0);

    let limits = Limits {
        file_cap: 1000,
        ..Limits::default()
    };
    let err = p
        .fetch_with(&item("big", AssetKind::Model3d), dest.path(), limits)
        .unwrap_err();
    assert!(err.to_string().contains("more than expected"), "{err}");
    assert_eq!(fs::read_dir(dest.path()).unwrap().count(), 0);

    let err = p
        .fetch(&item("missing", AssetKind::Model3d), dest.path())
        .unwrap_err();
    assert!(err.to_string().contains("404"), "{err}");
}

#[test]
fn fetches_a_texture_color_and_normal_map() {
    let server = FakeServer::start(|url, base| match url {
        "/files/brick" => Reply::ok(
            json!({
                "Diffuse": {"1k": {"jpg": {"url": format!("{base}/dl/brick_diff_1k.jpg")}, "png": {"url": format!("{base}/dl/brick_diff_1k.png")}},
                            "4k": {"png": {"url": format!("{base}/dl/no.png")}}},
                "nor_gl": {"1k": {"png": {"url": format!("{base}/dl/brick_nor_1k.png")}}},
                "Rough": {"1k": {"png": {"url": format!("{base}/dl/no2.png")}}},
            })
            .to_string(),
        ),
        "/dl/brick_diff_1k.png" => Reply::ok("D"),
        "/dl/brick_nor_1k.png" => Reply::ok("N"),
        _ => Reply::status(404),
    });
    let p = ph(&server.base);
    let dest = tempfile::tempdir().unwrap();
    let files = p
        .fetch(&item("brick", AssetKind::Image), dest.path())
        .unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        fs::read(dest.path().join("brick/brick_diff_1k.png")).unwrap(),
        b"D"
    );
    assert_eq!(
        fs::read(dest.path().join("brick/brick_nor_1k.png")).unwrap(),
        b"N"
    );
}

#[test]
fn fetches_an_hdri_and_falls_back_to_the_smallest_resolution() {
    let server = FakeServer::start(|url, base| match url {
        "/files/sky" => Reply::ok(
            json!({"hdri": {
                "8k": {"hdr": {"url": format!("{base}/dl/sky_8k.hdr")}},
                "2k": {"exr": {"url": format!("{base}/dl/sky_2k.exr")}, "hdr": {"url": format!("{base}/dl/sky_2k.hdr")}},
            }})
            .to_string(),
        ),
        "/dl/sky_2k.hdr" => Reply::ok("HDR"),
        _ => Reply::status(404),
    });
    let p = ph(&server.base);
    let dest = tempfile::tempdir().unwrap();
    let files = p
        .fetch(&item("sky", AssetKind::Image), dest.path())
        .unwrap();
    assert_eq!(files, [dest.path().join("sky/sky_2k.hdr")]);
}
