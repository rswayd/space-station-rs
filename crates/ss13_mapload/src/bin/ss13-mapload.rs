//! `ss13-mapload convert`: dm_types.json -> prototypes for the types the maps use.
//! `ss13-mapload load`: loads maps headless and prints the load report (timing, counts).
//! `ss13-mapload audit`: writes the asset license audit (`ss13_mapload::audit`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use anyhow::{Context as _, bail};
use bevy::prelude::{App, MinimalPlugins};
use ss13_assets::dmm::DmmMap;
use ss13_mapload::convert::{DmTypes, convert};
use ss13_mapload::spawner::dmm_loot_paths;
use ss13_mapload::{DEFAULT_AREA, DEFAULT_TURF, DmProtoPlugin, MapLoadPlugin};

const USAGE: &str = "\
usage:
  ss13-mapload convert [--source <repo>] [--assets <dir>] [--maps <dir or .dmm>]...
      reads <assets>/dm_types.json, writes <assets>/prototypes/dm_types.ron with every type the
      maps use (default maps: everything under <repo>/_maps/map_files and <repo>/_maps/shuttles,
      the lavaland ruins and this checkout's tests/cases)
      plus job outfit items,
      and <assets>/jobs.ron (joinable jobs, outfits, trim access, name lists)
      and <assets>/reagents.ron (tg reagents and reactions)
      and <assets>/antag.ron (uplink catalog, steal objective targets)
      and <assets>/cargo.ron (supply packs, exports), <assets>/research.ron (techweb)
      and <assets>/plants.ron (seeds, plant traits, hydroponics trays)
      and <assets>/mobs.ron (basic mob types)
      and <assets>/sfx.ron (SFX keys), <assets>/sound/ (referenced .ogg files, attributions)
  ss13-mapload load [--source <repo>] [--assets <dir>] [--seed <n>] [--raw] <map>...
      loads maps (names like MetaStation or .dmm paths) and prints the report; --raw skips the
      fidelity pass (spawners, undertile, smoothing); the seed defaults to $SS13_MAP_SEED
  ss13-mapload audit [--source <repo>] [--assets <dir>] [--out <file>]
      lists the copied sounds and atlas .dmi files whose attribution/license files name terms
      other than CC BY-SA 3.0 (ss13_mapload::audit) into <file> (default docs/ASSET-AUDIT.md)
<repo> defaults to $SS13_SOURCE, then ../BandaStation; <assets> to $SS13_ASSETS,
then assets/generated";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_default();
    let mut config = MapLoadPlugin {
        source: env_path("SS13_SOURCE", env!("SS13_SOURCE")),
        maps: Vec::new(),
        assets: env_path("SS13_ASSETS", "assets/generated"),
    };
    let mut map_dirs = Vec::new();
    let mut seed = ss13_mapload::MapSeed::from_env().0;
    let mut raw = false;
    let mut audit_out = PathBuf::from("docs/ASSET-AUDIT.md");
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--source" => config.source = value()?.into(),
            "--assets" => config.assets = value()?.into(),
            "--maps" if command == "convert" => map_dirs.push(PathBuf::from(value()?)),
            "--seed" if command == "load" => seed = value()?.parse().context("--seed")?,
            "--raw" if command == "load" => raw = true,
            "--out" if command == "audit" => audit_out = value()?.into(),
            _ if command == "load" && !arg.starts_with("--") => config.maps.push(arg),
            _ => bail!("unknown argument {arg}\n{USAGE}"),
        }
    }
    match command.as_str() {
        "convert" => convert_cmd(&config, map_dirs),
        "load" => load_cmd(config, seed, raw),
        "audit" => audit_cmd(&config, &audit_out),
        _ => bail!("{USAGE}"),
    }
}

fn audit_cmd(config: &MapLoadPlugin, out: &Path) -> anyhow::Result<()> {
    use ss13_mapload::audit;
    let rules = audit::parse_rules(audit::RULES)?;
    let assets = audit::copied_assets(&config.assets)?;
    let findings = audit::audit(
        &rules,
        assets.iter().map(|(a, root)| (a.as_str(), root.as_path())),
    );
    let commit = ss13_mapload::sound::source_commit(&config.source);
    std::fs::write(
        out,
        audit::report_markdown(&findings, assets.len(), &commit),
    )
    .with_context(|| format!("writing {}", out.display()))?;
    for risk in audit::RISKS {
        let n = findings.iter().filter(|f| f.risk == risk).count();
        println!("{risk}: {n}");
    }
    println!("{} assets checked -> {}", assets.len(), out.display());
    Ok(())
}

fn env_path(var: &str, default: &str) -> PathBuf {
    PathBuf::from(std::env::var(var).unwrap_or_else(|_| default.to_owned()))
}

fn convert_cmd(config: &MapLoadPlugin, mut map_dirs: Vec<PathBuf>) -> anyhow::Result<()> {
    let start = Instant::now();
    if map_dirs.is_empty() {
        map_dirs.push(config.source.join("_maps/map_files"));
        // Shuttle templates load with the station (`ss13_mapload::ShuttleTemplates`).
        map_dirs.push(config.source.join("_maps/shuttles"));
        // Lavaland ruins are pasted onto the mining level (`ss13_mapload::ruins`).
        map_dirs.push(config.source.join(ss13_mapload::ruins::LAVA_RUINS_DIR));
        // Virtual domains and their safehouses load mid-round (`ss13_game::science::bitrunning`).
        map_dirs.push(config.source.join("_maps/virtual_domains"));
        map_dirs.push(config.source.join("_maps/safehouses"));
        // So does the wizard's den (`ss13_game::antag::wizard`).
        map_dirs.push(
            config
                .source
                .join("_maps/templates/lazy_templates/wizard_den.dmm"),
        );
        // The functional test bots' case maps (`ss13_bots`) load on the same assets.
        let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/cases");
        if cases.is_dir() {
            map_dirs.push(cases);
        }
    }
    let mut maps = Vec::new();
    for dir in &map_dirs {
        collect_dmm(dir, &mut maps).with_context(|| format!("scanning {}", dir.display()))?;
    }
    let mut used = BTreeSet::from([DEFAULT_TURF.to_owned(), DEFAULT_AREA.to_owned()]);
    for map in &maps {
        let parsed =
            DmmMap::from_file(map).with_context(|| format!("parsing {}", map.display()))?;
        for prefab in &parsed.prefabs {
            used.extend(prefab.objects.iter().map(|o| o.path.clone()));
            used.extend(prefab.objects.iter().flat_map(dmm_loot_paths));
        }
    }
    let types = DmTypes::read(&config.assets.join("dm_types.json"))?;
    // What the game spawns by a constant path (`ss13_mapload::runtime`). A path the dump lacks
    // (renamed upstream) is reported, not fatal; the `runtime_types` asset test fails on it.
    for path in ss13_mapload::runtime::runtime_types() {
        if types.types.contains_key(path) {
            used.insert(path.to_owned());
        } else {
            println!("runtime type not in dm_types.json: {path}");
        }
    }
    used.extend(ss13_mapload::construct::table_runtime_types(&types));
    used.extend(ss13_mapload::construct::furniture_runtime_types(&types));
    used.extend(ss13_mapload::construct::door_assembly_runtime_types(&types));
    let mut jobs = ss13_mapload::jobs::resolve(&types);
    jobs.names = ss13_mapload::jobs::read_names(&config.source);
    // Outfit items are spawned at runtime, not placed on maps.
    // Unknown types are reported per job (`unresolved`) instead of failing the conversion.
    used.extend(
        jobs.outfit_items()
            .into_iter()
            .filter(|p| types.types.contains_key(*p))
            .map(str::to_owned),
    );
    let mut antag = ss13_mapload::antag::resolve(&types);
    for problem in ss13_mapload::antag::read_strings(&config.source, &mut antag) {
        println!("antag strings missing: {problem}");
    }
    // Uplink items are spawned at runtime, not placed on maps.
    used.extend(antag.item_types().into_iter().map(str::to_owned));
    // The nuclear operatives' outfits and gear are spawned at runtime.
    used.extend(
        antag
            .nukeop_types()
            .into_iter()
            .filter(|p| types.types.contains_key(*p))
            .map(str::to_owned),
    );
    let cargo = ss13_mapload::cargo::resolve(&types);
    // Supply crates, their contents and manifests are spawned at runtime.
    used.extend(
        cargo
            .spawned_types()
            .into_iter()
            .filter(|p| types.types.contains_key(*p))
            .map(str::to_owned),
    );
    let research = ss13_mapload::research::resolve(&types);
    // Printed designs and ejected sheets are spawned at runtime; every design shows its icon.
    used.extend(research.prototype_types().into_iter().map(str::to_owned));
    let modsuits = ss13_mapload::modsuit::resolve(&types);
    // MODsuit control units, their parts, modules, cores and cells are spawned at runtime.
    used.extend(
        modsuits
            .prototype_types()
            .into_iter()
            .filter(|p| types.types.contains_key(*p))
            .map(str::to_owned),
    );
    let mecha = ss13_mapload::mecha::resolve(&types);
    // Mechs, their parts, equipment, wreckages and cells are spawned at runtime.
    used.extend(
        mecha
            .prototype_types()
            .into_iter()
            .filter(|p| types.types.contains_key(*p))
            .map(str::to_owned),
    );
    let mut silicon = ss13_mapload::silicon::resolve(&types);
    if let Some(problem) = ss13_mapload::silicon::read_malf_flavor(&config.source, &mut silicon) {
        println!("malf flavor missing: {problem}");
    }
    // The cyborg models and their modules are spawned at runtime.
    used.extend(
        silicon
            .spawned_types()
            .into_iter()
            .filter(|p| types.types.contains_key(*p))
            .map(str::to_owned),
    );
    let mob_spawns = ss13_mapload::mob_spawn::resolve(&types);
    // The ghost roles' and corpses' outfits are spawned at runtime.
    used.extend(
        mob_spawns
            .item_types()
            .into_iter()
            .filter(|p| types.types.contains_key(*p))
            .map(str::to_owned),
    );
    let conversion = convert(&types, used.iter().map(String::as_str))?;
    let out = config.prototypes_dir().join("dm_types.ron");
    std::fs::create_dir_all(config.prototypes_dir())?;
    std::fs::write(&out, &conversion.ron).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "{} maps, {} used types -> {} prototypes in {} ({:.2?})",
        maps.len(),
        used.len(),
        conversion.prototypes,
        out.display(),
        start.elapsed()
    );
    println!(
        "declared values kept as inherited (not representable): {:?}",
        conversion.unsupported
    );
    let (chem, skipped) = ss13_mapload::reagents::chem_data(&types);
    let chem_out = config.assets.join(ss13_reagents::CHEM_FILE);
    std::fs::write(&chem_out, chem.to_ron())
        .with_context(|| format!("writing {}", chem_out.display()))?;
    println!(
        "{} reagents, {} reactions -> {} (not converted: {} randomized, {} with their own \
         pre_reaction_other_checks, {} with non-constant reagent lists)",
        chem.reagents.len(),
        chem.reactions.len(),
        chem_out.display(),
        skipped.randomized,
        skipped.own_checks.len(),
        skipped.unreadable.len()
    );
    let mob_spawn_out = config.assets.join(ss13_mapload::mob_spawn::MOB_SPAWN_FILE);
    std::fs::write(&mob_spawn_out, mob_spawns.to_ron())
        .with_context(|| format!("writing {}", mob_spawn_out.display()))?;
    println!(
        "{} mob spawners -> {}",
        mob_spawns.spawns.len(),
        mob_spawn_out.display()
    );
    let antag_out = config.assets.join(ss13_mapload::antag::ANTAG_FILE);
    std::fs::write(&antag_out, antag.to_ron())
        .with_context(|| format!("writing {}", antag_out.display()))?;
    println!(
        "{} uplink items, {} steal targets, {} traitor employers -> {}",
        antag.uplink_items.len(),
        antag.steal_items.len(),
        antag.traitor_flavor.len(),
        antag_out.display()
    );
    let cargo_out = config.assets.join(ss13_mapload::cargo::CARGO_FILE);
    std::fs::write(&cargo_out, cargo.to_ron())
        .with_context(|| format!("writing {}", cargo_out.display()))?;
    println!(
        "{} supply packs, {} exports -> {}",
        cargo.packs.len(),
        cargo.exports.len(),
        cargo_out.display()
    );
    let research_out = config.assets.join(ss13_mapload::research::RESEARCH_FILE);
    std::fs::write(&research_out, research.to_ron())
        .with_context(|| format!("writing {}", research_out.display()))?;
    println!(
        "{} techweb nodes, {} designs, {} silo materials, {} lathe types -> {}",
        research.nodes.len(),
        research.designs.len(),
        research.materials.len(),
        research.lathes.len(),
        research_out.display()
    );
    let handbook = ss13_mapload::atmos_handbook::resolve(&types);
    let handbook_out = config
        .assets
        .join(ss13_mapload::atmos_handbook::HANDBOOK_FILE);
    std::fs::write(
        &handbook_out,
        ss13_mapload::atmos_handbook::to_ron(&handbook),
    )
    .with_context(|| format!("writing {}", handbook_out.display()))?;
    println!(
        "{} gases, {} reactions -> {}",
        handbook.gas_info.len(),
        handbook.reaction_info.len(),
        handbook_out.display()
    );
    let mut sfx = ss13_mapload::sound::resolve_sfx(&types);
    sfx.vox = ss13_mapload::sound::resolve_vox(&config.source);
    let sfx_out = config.assets.join(ss13_mapload::sound::SFX_FILE);
    std::fs::write(&sfx_out, sfx.to_ron())
        .with_context(|| format!("writing {}", sfx_out.display()))?;
    let mut sounds =
        ss13_mapload::sound::referenced(&types, &sfx, conversion.types.iter().map(String::as_str));
    sounds.extend(modsuits.sounds().into_iter().map(str::to_owned));
    sounds.extend(mecha.sounds().into_iter().map(str::to_owned));
    let copied = ss13_mapload::sound::copy_sounds(
        &config.source,
        &config.assets,
        &sounds,
        &ss13_mapload::sound::source_commit(&config.source),
    )?;
    println!(
        "{} sfx keys, {} VOX words -> {}; {} sound files ({:.1} MB, {} attribution files) -> {}",
        sfx.sfx.len(),
        sfx.vox.len(),
        sfx_out.display(),
        copied.copied,
        copied.bytes as f64 / 1e6,
        copied.attributions,
        config.assets.join(ss13_mapload::sound::SOUND_DIR).display()
    );
    for missing in &copied.missing {
        println!("  sound file missing in the source: {missing}");
    }
    let modsuit_out = config.assets.join(ss13_mapload::modsuit::MODSUIT_FILE);
    std::fs::write(&modsuit_out, modsuits.to_ron())
        .with_context(|| format!("writing {}", modsuit_out.display()))?;
    println!(
        "{} MOD themes, {} control units, {} modules -> {}",
        modsuits.themes.len(),
        modsuits.suits.len(),
        modsuits.modules.len(),
        modsuit_out.display()
    );
    let mecha_out = config.assets.join(ss13_mapload::mecha::MECHA_FILE);
    std::fs::write(&mecha_out, mecha.to_ron())
        .with_context(|| format!("writing {}", mecha_out.display()))?;
    println!(
        "{} mechs, {} equipment types, {} chassis -> {}",
        mecha.mechs.len(),
        mecha.equipment.len(),
        mecha.constructions.len(),
        mecha_out.display()
    );
    let silicon_out = config.assets.join(ss13_mapload::silicon::SILICON_FILE);
    std::fs::write(&silicon_out, silicon.to_ron())
        .with_context(|| format!("writing {}", silicon_out.display()))?;
    println!(
        "{} lawsets, {} cyborg models -> {}",
        silicon.lawsets.len(),
        silicon.models.len(),
        silicon_out.display()
    );
    let mobs = ss13_mapload::mobs::resolve(
        &types,
        &ss13_mapload::mobs::RuVerbs::read(&config.source),
        &ss13_mapload::mobs::read_factions(&config.source),
    );
    let mobs_out = config.assets.join(ss13_mapload::mobs::MOBS_FILE);
    std::fs::write(&mobs_out, mobs.to_ron())
        .with_context(|| format!("writing {}", mobs_out.display()))?;
    println!(
        "{} basic mob types -> {}",
        mobs.mobs.len(),
        mobs_out.display()
    );
    let restaurant = ss13_mapload::restaurant::resolve(&types, &config.source);
    let restaurant_out = config
        .assets
        .join(ss13_mapload::restaurant::RESTAURANT_FILE);
    std::fs::write(&restaurant_out, restaurant.to_ron())
        .with_context(|| format!("writing {}", restaurant_out.display()))?;
    println!(
        "{} venues, {} customer types -> {}",
        restaurant.venues.len(),
        restaurant.customers.len(),
        restaurant_out.display()
    );
    let cards = ss13_mapload::service::CardStrings::read_source(&config.source);
    let cards_out = config.assets.join(ss13_mapload::service::CARDS_FILE);
    std::fs::write(&cards_out, cards.to_ron())
        .with_context(|| format!("writing {}", cards_out.display()))?;
    println!(
        "{} + {} CAS cards -> {}",
        cards.cas_white.len(),
        cards.cas_black.len(),
        cards_out.display()
    );
    let plants = ss13_mapload::plants::resolve(&types);
    let plants_out = config.assets.join(ss13_mapload::plants::PLANTS_FILE);
    std::fs::write(&plants_out, plants.to_ron())
        .with_context(|| format!("writing {}", plants_out.display()))?;
    println!(
        "{} seeds, {} plant traits, {} tray types -> {}",
        plants.seeds.len(),
        plants.traits.len(),
        plants.trays.len(),
        plants_out.display()
    );
    let jobs_out = config.assets.join(ss13_mapload::jobs::JOBS_FILE);
    std::fs::write(&jobs_out, jobs.to_ron())
        .with_context(|| format!("writing {}", jobs_out.display()))?;
    let resolved = jobs.jobs.iter().filter(|j| j.unresolved.is_empty()).count();
    println!(
        "{} joinable jobs in {} departments, {resolved} fully resolved -> {}",
        jobs.jobs.len(),
        jobs.departments.len(),
        jobs_out.display()
    );
    for job in jobs.jobs.iter().filter(|j| !j.unresolved.is_empty()) {
        println!("  {}: {}", job.title, job.unresolved.join("; "));
    }
    println!(
        "names: {} male, {} female first names, {} last names",
        jobs.names.first_male.len(),
        jobs.names.first_female.len(),
        jobs.names.last.len()
    );
    Ok(())
}

fn collect_dmm(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.extension().is_some_and(|e| e == "dmm") {
        out.push(path.to_owned());
        return Ok(());
    }
    let mut entries: Vec<_> = std::fs::read_dir(path)?
        .map(|e| e.map(|e| e.path()))
        .collect::<Result<_, _>>()?;
    entries.sort();
    for entry in entries {
        if entry.is_dir() || entry.extension().is_some_and(|e| e == "dmm") {
            collect_dmm(&entry, out)?;
        }
    }
    Ok(())
}

fn load_cmd(config: MapLoadPlugin, seed: u64, raw: bool) -> anyhow::Result<()> {
    if config.maps.is_empty() {
        bail!("no maps given\n{USAGE}");
    }
    let start = Instant::now();
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        ss13_world::WorldPlugin,
        DmProtoPlugin,
        ss13_proto::ProtoPlugin {
            dir: config.prototypes_dir(),
        },
    ));
    app.finish();
    let protos = start.elapsed();
    let manifest = ss13_mapload::read_manifest(&config.assets)?;
    let report = ss13_mapload::load_maps(app.world_mut(), &config.map_paths(), &manifest)?;
    println!("prototypes loaded in {protos:.2?}, manifest read");
    println!("{report}");
    for (key, n) in &report.missing_icons {
        println!("  missing {key}: {n}");
    }
    if !raw {
        let fidelity = ss13_mapload::fidelity::apply(app.world_mut(), &manifest, seed);
        println!("{fidelity}");
        println!("load + fidelity: {:.2?}", report.elapsed + fidelity.elapsed);
    }
    Ok(())
}
