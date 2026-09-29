use std::fs;
use std::path::Path;

use crate::assets::AssetKind;
use crate::library::local_folder::LocalFolder;
use crate::library::{LibraryItem, LibraryProvider, LibraryQuery};

fn touch(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn search(text: &str, kind: Option<AssetKind>, limit: Option<u32>) -> anyhow::Result<Vec<LibraryItem>> {
    LocalFolder.search(&LibraryQuery {
        text: text.into(),
        kind,
        limit,
    })
}

fn list(folder: &Path) -> Vec<LibraryItem> {
    search(&folder.to_string_lossy(), None, None).unwrap()
}

fn titles(items: &[LibraryItem]) -> Vec<&str> {
    items.iter().map(|i| i.title.as_str()).collect()
}

#[test]
fn cc0_pack_is_recognised_with_kinds_and_source() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("kenney_platformer");
    touch(
        &pack.join("License.txt"),
        "Creative Commons Zero, CC0 1.0 Universal\n\nYou may use these assets freely.\n",
    );
    touch(&pack.join("Sprites/player.png"), "x");
    touch(&pack.join("Sounds/jump.ogg"), "x");
    touch(&pack.join("Models/tree.glb"), "x");
    touch(&pack.join("Fonts/pixel.ttf"), "x");
    touch(&pack.join("readme.html"), "x");
    let items = list(&pack);
    assert_eq!(items.len(), 4);
    let player = items.iter().find(|i| i.title == "player").unwrap();
    assert_eq!(player.kind, AssetKind::Image);
    assert_eq!(player.provider, "local_folder");
    assert_eq!(player.id, pack.canonicalize().unwrap().join("Sprites/player.png").to_string_lossy());
    assert_eq!(player.license.name.as_deref(), Some("CC0-1.0"));
    assert_eq!(
        player.license.source.as_deref(),
        Some("My own files: kenney_platformer")
    );
    let kinds: Vec<_> = items.iter().map(|i| i.kind).collect();
    assert!(kinds.contains(&AssetKind::Audio));
    assert!(kinds.contains(&AssetKind::Model3d));
    assert!(kinds.contains(&AssetKind::Font));
    // Every file in the pack (in subfolders too) shares the pack license.
    assert!(items.iter().all(|i| i.license.name.as_deref() == Some("CC0-1.0")));
}

#[test]
fn cc_by_pack_with_an_author_line() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("quaternius");
    touch(
        &pack.join("LICENSE"),
        "Created by Jane Doe\nLicensed under CC-BY 4.0 (Attribution 4.0 International).\n",
    );
    touch(&pack.join("rock.png"), "x");
    let items = list(&pack);
    assert_eq!(items[0].license.name.as_deref(), Some("CC-BY-4.0"));
    assert_eq!(items[0].license.author.as_deref(), Some("Jane Doe"));
}

#[test]
fn other_known_and_unknown_licenses() {
    let cases = [
        ("Licensed CC-BY 3.0", "CC-BY-3.0"),
        ("CC-BY-SA 3.0 Unported", "CC-BY-SA-3.0"),
        ("Creative Commons Attribution-ShareAlike 4.0", "CC-BY-SA-4.0"),
        ("MIT License\n\nCopyright (c) 2024", "MIT"),
        ("Apache License\nVersion 2.0", "Apache-2.0"),
        ("\n\n  Free for hobby use only, ask first.\nmore", "Free for hobby use only, ask first."),
    ];
    for (text, want) in cases {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("license.txt"), text);
        touch(&dir.path().join("a.png"), "x");
        let items = list(dir.path());
        assert_eq!(items[0].license.name.as_deref(), Some(want), "for {text:?}");
    }
    // A very long first line is trimmed to 80 characters.
    let dir = tempfile::tempdir().unwrap();
    touch(&dir.path().join("COPYING"), &"z".repeat(300));
    touch(&dir.path().join("a.png"), "x");
    assert_eq!(list(dir.path())[0].license.name.as_ref().unwrap().chars().count(), 80);
}

#[test]
fn pack_with_no_license_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    touch(&dir.path().join("sprites/a.png"), "x");
    let items = list(dir.path());
    assert_eq!(items[0].license.name, None);
    assert_eq!(items[0].license.author, None);
    assert!(items[0].license.source.as_ref().unwrap().starts_with("My own files: "));
}

#[test]
fn license_is_found_in_up_to_two_parents_but_not_above_the_chosen_folder() {
    let dir = tempfile::tempdir().unwrap();
    // Licence two levels above the files, inside the chosen folder.
    let pack = dir.path().join("pack");
    touch(&pack.join("Licence.txt"), "CC0");
    touch(&pack.join("a/b/deep.png"), "x");
    // Three levels above: too far.
    touch(&pack.join("c/d/e/toofar.png"), "x");
    let items = list(&pack);
    let deep = items.iter().find(|i| i.title == "deep").unwrap();
    assert_eq!(deep.license.name.as_deref(), Some("CC0-1.0"));
    let far = items.iter().find(|i| i.title == "toofar").unwrap();
    assert_eq!(far.license.name, None);

    // A license above the folder the person chose is not used.
    let outer = dir.path().join("outer");
    touch(&outer.join("LICENSE"), "MIT License");
    touch(&outer.join("inner/x.png"), "x");
    let items = list(&outer.join("inner"));
    assert_eq!(items[0].license.name, None);
}

#[test]
fn nested_folders_hidden_and_macosx_are_handled() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    touch(&root.join("a.png"), "x");
    touch(&root.join("one/two/three/four/five/six/ok.png"), "x"); // depth 6
    touch(&root.join("one/two/three/four/five/six/seven/toodeep.png"), "x");
    touch(&root.join(".hidden/secret.png"), "x");
    touch(&root.join("__MACOSX/._a.png"), "x");
    touch(&root.join("visible/.dot.png"), "x");
    touch(&root.join("visible/b.png"), "x");
    let items = list(root);
    assert_eq!(titles(&items), ["a", "ok", "b"]);
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    touch(&outside.path().join("stolen.png"), "x");
    let root = dir.path().join("pack");
    touch(&root.join("real.png"), "x");
    std::os::unix::fs::symlink(outside.path(), root.join("linked_dir")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("stolen.png"), root.join("linked.png")).unwrap();
    let items = list(&root);
    assert_eq!(titles(&items), ["real"]);
    // And fetch refuses a symlink even when given its path.
    let link = LibraryItem {
        id: root.join("linked.png").to_string_lossy().into_owned(),
        ..items[0].clone()
    };
    let dest = tempfile::tempdir().unwrap();
    assert!(LocalFolder.fetch(&link, dest.path()).is_err());
}

#[test]
fn name_filter_kind_and_limit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for n in ["tree_a.png", "tree_b.png", "rock.png", "Trees/leaf.png"] {
        touch(&root.join(n), "x");
    }
    touch(&root.join("tree.wav"), "x");
    let folder = root.to_string_lossy().into_owned();
    let items = search(&format!("{folder}|TREE"), None, None).unwrap();
    assert_eq!(titles(&items), ["leaf", "tree", "tree_a", "tree_b"]);
    let images = search(&format!("{folder}|tree"), Some(AssetKind::Image), None).unwrap();
    assert_eq!(images.len(), 3);
    assert_eq!(search(&folder, None, Some(2)).unwrap().len(), 2);
    assert_eq!(search(&folder, None, None).unwrap().len(), 5);
}

#[test]
fn bad_folders_get_friendly_errors() {
    let err = search("/no/such/folder/anywhere", None, None).unwrap_err();
    assert!(err.to_string().contains("Couldn't find that folder"));
    let dir = tempfile::tempdir().unwrap();
    touch(&dir.path().join("a.png"), "x");
    let err = search(&dir.path().join("a.png").to_string_lossy(), None, None).unwrap_err();
    assert!(err.to_string().contains("not a folder"));
    assert!(search("  ", None, None).is_err());
}

#[test]
fn fetch_copies_without_overwriting() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("pack");
    touch(&pack.join("hero.png"), "PACK");
    let item = list(&pack).remove(0);
    let dest = tempfile::tempdir().unwrap();
    let dest_dir = dest.path().join("new/place");
    let first = LocalFolder.fetch(&item, &dest_dir).unwrap();
    assert_eq!(first, [dest_dir.join("hero.png")]);
    fs::write(&first[0], "MINE").unwrap();
    let second = LocalFolder.fetch(&item, &dest_dir).unwrap();
    assert_eq!(second, [dest_dir.join("hero-2.png")]);
    assert_eq!(fs::read_to_string(&first[0]).unwrap(), "MINE");
    assert_eq!(fs::read_to_string(&second[0]).unwrap(), "PACK");
}

#[test]
fn fetch_refuses_things_it_could_not_have_listed() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("pack");
    touch(&pack.join("hero.png"), "x");
    touch(&pack.join("notes.txt"), "x");
    touch(&pack.join(".h.png"), "x");
    let base = list(&pack).remove(0);
    let dest = tempfile::tempdir().unwrap();
    for id in [
        pack.join("missing.png"),
        pack.join("notes.txt"),
        pack.join(".h.png"),
        pack.clone(),
        pack.join("../pack/hero.png"),
        "relative/hero.png".into(),
    ] {
        let item = LibraryItem {
            id: id.to_string_lossy().into_owned(),
            ..base.clone()
        };
        assert!(LocalFolder.fetch(&item, dest.path()).is_err(), "{id:?}");
    }
    assert_eq!(fs::read_dir(dest.path()).unwrap().count(), 0);
}

#[test]
fn fetch_copies_a_gltfs_bin_and_textures() {
    let dir = tempfile::tempdir().unwrap();
    let pack = dir.path().join("pack");
    touch(
        &pack.join("models/tree.gltf"),
        r#"{"buffers":[{"uri":"tree.bin"}],
            "images":[{"uri":"textures/leaf%20a.png"},{"uri":"data:image/png;base64,AAAA"},
                      {"uri":"https://example.com/x.png"},{"uri":"../secret.png"},{"uri":"gone.png"}]}"#,
    );
    touch(&pack.join("models/tree.bin"), "BIN");
    touch(&pack.join("models/textures/leaf a.png"), "PNG");
    touch(&pack.join("secret.png"), "SECRET");
    let item = list(&pack)
        .into_iter()
        .find(|i| i.title == "tree")
        .unwrap();
    let dest = tempfile::tempdir().unwrap();
    let files = LocalFolder.fetch(&item, dest.path()).unwrap();
    assert_eq!(files.len(), 3);
    assert_eq!(fs::read_to_string(dest.path().join("tree.bin")).unwrap(), "BIN");
    assert_eq!(
        fs::read_to_string(dest.path().join("textures/leaf a.png")).unwrap(),
        "PNG"
    );
    assert!(!dest.path().join("secret.png").exists());
    assert!(!dest.path().parent().unwrap().join("secret.png").exists());

    // A second import goes into its own numbered folder so links keep working.
    let again = LocalFolder.fetch(&item, dest.path()).unwrap();
    assert_eq!(again.len(), 3);
    assert!(again.iter().all(|p| p.starts_with(dest.path().join("tree-2"))));
    assert_eq!(fs::read_to_string(dest.path().join("tree.bin")).unwrap(), "BIN");
}
