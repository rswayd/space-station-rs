use std::path::{Path, PathBuf};

use serde_json::json;
use ss13_dm_extract::{ACCEPTANCE_MAP_DIRS, ALL_MAPS_DIR, check_maps, extract, load};

const FIXTURE_DME: &str = r#"
#define MY_LAYER (2 + 1000)
#define MY_PLANE -4
#ifdef CBT
#define MAP_SWITCH(compile_time, map_time) ##compile_time
#else
#define MAP_SWITCH(compile_time, map_time) ##map_time
#endif
#include "code\things.dm"
"#;

const FIXTURE_DM: &str = r#"
/obj/base
	name = "base"
	layer = MY_LAYER
	plane = MY_PLANE
	icon = 'icons/base.dmi'
	var/list/stuff = list("a" = 1)

/obj/base/child
	desc = "child"
	pixel_x = 0.5
	color = list(1, "b" = /obj/base)

/obj/base/weird
	color = pick("a", "b") // not constant
	icon_state = MAP_SWITCH("runtime", "preview")

/obj/base/Initialize(mapload)
	. = ..()
	// AddElement(/datum/element/undertile) in a comment does not count
	if(stuff)
		AddElement( /datum/element/undertile, TRAIT_X)

/obj/base/child/proc/other()
	RemoveElement(/datum/element/undertile)

/obj/effect/mapping_helpers/airlock/access/proc/get_access()
	var/list/access_list = list()
	return access_list

/obj/effect/mapping_helpers/airlock/access/all/eva/get_access()
	var/list/access_list = ..()
	access_list += "eva"
	return access_list
"#;

/// An access helper whose `get_access()` has logic the extractor does not read.
const BAD_HELPER_DM: &str = r#"
/obj/effect/mapping_helpers/airlock/access/all/odd/get_access()
	var/list/access_list = ..()
	if(prob(50))
		access_list += "eva"
	return access_list
"#;

fn fixture(test: &str) -> PathBuf {
    fixture_with(&format!("dm_extract_fixture/{test}"), "")
}

fn fixture_with(name: &str, extra_dm: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(dir.join("code")).unwrap();
    std::fs::create_dir_all(dir.join("icons")).unwrap();
    std::fs::write(dir.join("tgstation.dme"), FIXTURE_DME).unwrap();
    std::fs::write(
        dir.join("code/things.dm"),
        format!("{FIXTURE_DM}{extra_dm}"),
    )
    .unwrap();
    std::fs::write(dir.join("icons/base.dmi"), b"").unwrap();
    dir
}

#[test]
fn resolves_inherited_constant_values() {
    let env = load(&fixture("resolves_inherited_constant_values")).unwrap();
    let (dump, non_const) = extract(&env);

    let child = &dump.types["/obj/base/child"];
    assert_eq!(child.parent.as_deref(), Some("/obj/base"));
    assert_eq!(child.vars["name"], json!("base"));
    assert_eq!(child.vars["desc"], json!("child"));
    assert_eq!(child.vars["layer"], json!(1002));
    assert_eq!(child.vars["plane"], json!(-4));
    assert_eq!(child.vars["pixel_x"], json!(0.5));
    assert_eq!(child.vars["icon"], json!({ "resource": "icons/base.dmi" }));
    // Built like tg's `tools/build` (`-DCBT`): the runtime branch of `MAP_SWITCH`.
    assert_eq!(
        dump.types["/obj/base/weird"].vars["icon_state"],
        json!("runtime")
    );
    assert_eq!(
        child.vars["color"],
        json!({ "list": [1, { "assoc": ["b", { "path": "/obj/base" }] }] })
    );
    assert_eq!(child.vars["dir"], json!(2)); // builtin default
    assert_eq!(dump.schema, 3);
    assert_eq!(dump.types["/obj/base"].procs, ["Initialize"]);
    assert_eq!(dump.types["/obj/base/child"].procs, ["other"]);
    let calls: Vec<_> = dump.element_calls["/datum/element/undertile"]
        .iter()
        .map(|c| (c.owner.as_str(), c.proc.as_str(), c.call))
        .collect();
    assert_eq!(
        calls,
        [
            ("/obj/base", "Initialize", "AddElement"),
            ("/obj/base/child", "other", "RemoveElement")
        ]
    );
    let names = |path: &str| {
        dump.types[path]
            .declared_vars
            .keys()
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(names("/obj/base/child"), ["color", "desc", "pixel_x"]);
    assert_eq!(
        names("/obj/base"),
        ["icon", "layer", "name", "plane", "stuff"]
    );
    let base = &dump.types["/obj/base"].declared_vars;
    assert_eq!(base["layer"], json!(1002));
    assert_eq!(base["stuff"], json!({ "list": [{ "assoc": ["a", 1] }] }));
    assert_eq!(child.declared_vars["pixel_x"], json!(0.5));
    assert_eq!(
        dump.types["/obj/base/weird"].declared_vars["color"],
        json!({ "expr": "pick(\"a\", \"b\")" })
    );
    assert_eq!(dump.types["/obj"].parent.as_deref(), Some("/atom/movable"));

    assert_eq!(
        dump.types["/obj/base/weird"].vars["color"],
        json!({ "expr": "pick(\"a\", \"b\")" })
    );
    assert_eq!(non_const.get("color"), Some(&1));
}

#[test]
fn unreadable_access_helper_refuses_the_dump() {
    let env = load(&fixture("unreadable_access_helper_refuses_the_dump")).unwrap();
    let (dump, _) = extract(&env);
    assert_eq!(
        dump.types["/obj/effect/mapping_helpers/airlock/access/all/eva"].get_access,
        ["eva"]
    );
    dump.ensure_complete().unwrap();

    let env = load(&fixture_with("dm_extract_bad_helper", BAD_HELPER_DM)).unwrap();
    let (dump, non_const) = extract(&env);
    assert_eq!(non_const.get("get_access"), Some(&1));
    let err = dump.ensure_complete().unwrap_err().to_string();
    assert!(
        err.contains("/obj/effect/mapping_helpers/airlock/access/all/odd"),
        "{err}"
    );
}

const TRIM_DME: &str = r#"
#define ACCESS_A "a"
#define ACCESS_B "b"
#define ACCESS_C "c"
#define FLAG_ONE (1 << 0)
#define FLAG_TWO (1 << 1)
#define ONE_ACCESS list(ACCESS_A, ACCESS_B)
#define TWO_ACCESS list(ACCESS_C)
#include "code\trims.dm"
"#;

const TRIM_DM: &str = r#"
/datum/controller/subsystem/id_access
	var/list/accesses_by_flag = list()
	var/list/flags_by_access = list()
	var/list/access_flag_string_by_flag = list()

/datum/controller/subsystem/id_access/proc/setup_access_flags()
	accesses_by_flag["[FLAG_ONE]"] = ONE_ACCESS
	for(var/access in accesses_by_flag["[FLAG_ONE]"])
		flags_by_access |= list("[access]" = FLAG_ONE)

	accesses_by_flag["[FLAG_TWO]"] = TWO_ACCESS
	for(var/access in accesses_by_flag["[FLAG_TWO]"])
		flags_by_access |= list("[access]" = FLAG_TWO)

	access_flag_string_by_flag["[FLAG_ONE]"] = "One"

/datum/id_trim
	var/list/minimal_access = list()
	var/list/extra_access = list()

/datum/id_trim/New()
	if(minimal_access)
		minimal_access += list()

/datum/id_trim/boss/New()
	. = ..()
	extra_access |= (SSid_access.get_flag_access_list(FLAG_ONE) + SSid_access.get_flag_access_list(FLAG_TWO))
	minimal_access |= SSid_access.get_flag_access_list(FLAG_TWO)
	return ..()

/datum/id_trim/odd/New()
	. = ..()
	extra_access |= SSid_access.get_flag_access_list(pick(FLAG_ONE, FLAG_TWO))
"#;

/// Trim access computed in procs: the flag groups of `SSid_access` and a trim `New()` adding them.
#[test]
fn trim_access_flags_from_procs() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dm_extract_trims");
    std::fs::create_dir_all(dir.join("code")).unwrap();
    std::fs::write(dir.join("tgstation.dme"), TRIM_DME).unwrap();
    std::fs::write(dir.join("code/trims.dm"), TRIM_DM).unwrap();
    let env = load(&dir).unwrap();
    let (dump, non_const) = extract(&env);
    assert_eq!(dump.access_flags["1"], ["a", "b"]);
    assert_eq!(dump.access_flags["2"], ["c"]);
    let boss = &dump.types["/datum/id_trim/boss"].new_access_flags;
    assert_eq!(boss["extra_access"], [1, 2]);
    assert_eq!(boss["minimal_access"], [2]);
    // The root `New()` has logic but no flags: not reported. `odd` picks at runtime: reported.
    assert!(dump.types["/datum/id_trim"].new_access_flags.is_empty());
    assert!(dump.types["/datum/id_trim/odd"].new_access_flags.is_empty());
    assert_eq!(non_const.get("new_access_flags"), Some(&1));
    assert_eq!(non_const.get("access_flags"), None);
}

/// Acceptance: every type placed on MetaStation and tramstation exists in the source object tree.
/// Needs the BYOND checkout: `SS13_SOURCE` (default `../BandaStation`).
#[test]
#[ignore = "needs the BYOND source checkout"]
fn acceptance_maps_resolve() {
    let source = std::env::var_os("SS13_SOURCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("SS13_SOURCE")));
    let env = load(&source).unwrap();
    let dirs: Vec<_> = ACCEPTANCE_MAP_DIRS.iter().map(|d| source.join(d)).collect();
    let check = check_maps(&env.tree, &dirs).unwrap();
    assert!(!check.maps.is_empty());
    assert!(
        check.unresolved.is_empty(),
        "unresolved: {:?}",
        check.unresolved
    );
    let (dump, _) = extract(&env);
    for path in &check.used {
        let entry = &dump.types[path];
        for var in ["layer", "plane"] {
            assert!(
                entry.vars[var].is_number(),
                "{path}.{var} = {}",
                entry.vars[var]
            );
        }
    }
}

/// Every map under `_maps/map_files`: prints the unresolved types instead of failing on them,
/// because fork-only or abandoned maps may reference removed types; the numbers go to the report.
#[test]
#[ignore = "needs the BYOND source checkout"]
fn all_maps_report() {
    let source = std::env::var_os("SS13_SOURCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("SS13_SOURCE")));
    let env = load(&source).unwrap();
    let check = check_maps(&env.tree, &[source.join(ALL_MAPS_DIR)]).unwrap();
    println!(
        "maps: {}, used types: {}, unresolved: {}",
        check.maps.len(),
        check.used.len(),
        check.unresolved.len()
    );
    for (path, map) in &check.unresolved {
        println!("  {path} (in {})", map.display());
    }
    assert!(check.maps.len() > 2);
}

const POPULATE_DM: &str = r#"
/obj/item/a
/obj/item/b
/obj/item/c
/obj/structure/closet
	var/empty = FALSE
	var/mask = /obj/item/a
	var/count_var = 2

/obj/structure/closet/proc/PopulateContents()
	return

/obj/structure/closet/x/PopulateContents()
	..()
	new /obj/item/a(src)
	new /obj/item/a(loc)
	for(var/i in 1 to 3)
		new /obj/item/b(src)
	if(prob(40))
		new /obj/item/c(src)
	else
		new /obj/item/a(src)
	if(empty)
		return
	var/list/items_inside = list(/obj/item/a = 2, /obj/item/b = 1)
	generate_items_inside(items_inside, src)
	switch(pick_weight(list("one" = 3, "none" = 1)))
		if("one")
			new /obj/item/c(src)
	var/T = pick(/obj/item/a, /obj/item/b)
	new T(src)
	if(!isnull(mask))
		new mask(src)
	SSwardrobe.provide_type(/obj/item/c, src)
	for(var/i in 1 to count_var)
		new /obj/item/b(src)
	var/obj/item/c/thing = new(src, 5)
	thing.set_color("red")
	name = "renamed"
	update_appearance()
	for(var/obj/item/thing in src)
		new /obj/item/a(src)
"#;

#[test]
fn populate_contents_lowered() {
    use ss13_dm_extract::populate::Fill::{self, *};
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dm_extract_populate");
    std::fs::create_dir_all(dir.join("code")).unwrap();
    std::fs::write(dir.join("tgstation.dme"), "#include \"code\\things.dm\"\n").unwrap();
    std::fs::write(dir.join("code/things.dm"), POPULATE_DM).unwrap();
    let env = load(&dir).unwrap();
    let (dump, _) = extract(&env);
    let new = |p: &str| New(p.to_owned());
    let fill = &dump.populate["/obj/structure/closet/x"]["PopulateContents"];
    let expected: Vec<Fill> = vec![
        Parent,
        new("/obj/item/a"),
        Unsupported("new /obj/item/a(loc)".into()),
        Repeat {
            count: 3,
            body: vec![new("/obj/item/b")],
        },
        Prob {
            percent: 40.0,
            then: vec![new("/obj/item/c")],
            otherwise: vec![new("/obj/item/a")],
        },
        IfVar {
            var: "empty".into(),
            then: vec![Return],
            otherwise: vec![],
        },
        Repeat {
            count: 2,
            body: vec![new("/obj/item/a")],
        },
        new("/obj/item/b"),
        Pick {
            arms: vec![(3.0, vec![new("/obj/item/c")]), (1.0, vec![])],
        },
        Pick {
            arms: vec![
                (1.0, vec![new("/obj/item/a")]),
                (1.0, vec![new("/obj/item/b")]),
            ],
        },
        IfVar {
            var: "mask".into(),
            then: vec![NewVar("mask".into())],
            otherwise: vec![],
        },
        new("/obj/item/c"),
        RepeatVar {
            var: "count_var".into(),
            body: vec![new("/obj/item/b")],
        },
        new("/obj/item/c"),
        Unsupported("for(var/obj/item/thing in src)".into()),
    ];
    assert_eq!(fill, &expected);
    assert_eq!(
        dump.populate["/obj/structure/closet"]["PopulateContents"],
        [Return]
    );
}
