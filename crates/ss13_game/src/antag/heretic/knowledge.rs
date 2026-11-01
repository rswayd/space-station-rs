//! The heretic's knowledge shops (tg `heretic_shops`, `_heretic_paths.dm`): the knowledge table
//! types ([`super::data`] holds the values), the shop entries with their `HKT_NEXT`/`HKT_BAN` ids,
//! the start shop, a path's tree, its drafts and shop, what is researchable, and the transmutation
//! recipe matching (`/obj/effect/heretic_rune/proc/do_ritual`). Pure: no world access.

use super::data::{KNOWLEDGE, PATHS};

/// The knowledge's family (its parent type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `/datum/heretic_knowledge/spell`: grants a spell with charges.
    Spell,
    /// `/datum/heretic_knowledge/limited_amount/starting`: picks the path, makes its blade.
    Starting,
    /// `/datum/heretic_knowledge/limited_amount`.
    Limited,
    BladeUpgrade,
    Summon,
    /// `/datum/heretic_knowledge/ultimate`: the ascension.
    Ultimate,
    KnowledgeRitual,
    Curse,
    Armor,
    Other,
}

/// One `/datum/heretic_knowledge` type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnowledgeDef {
    pub path: &'static str,
    pub kind: Kind,
    pub name: &'static str,
    pub desc: &'static str,
    pub gain_text: &'static str,
    pub transmute_text: &'static str,
    pub notice: &'static str,
    pub cost: u32,
    pub priority: i32,
    /// 0: never drafted nor sold.
    pub drafting_tier: u32,
    pub shop_only: bool,
    pub starting: bool,
    /// `is_final_knowledge`: disables blade breaking.
    pub is_final: bool,
    /// `required_atoms`: (any of these types, amount).
    pub required: &'static [(&'static [&'static str], u32)],
    pub results: &'static [&'static str],
    /// `limited_amount` `limit`.
    pub limit: u32,
    /// Spells: `max_charges` (`u32::MAX` for INFINITY).
    pub max_charges: u32,
    /// Spells: `action_to_add` and its name.
    pub action: &'static str,
    pub action_name: &'static str,
    /// `get_icon_of_knowledge`: (dmi, state).
    pub icon: (&'static str, &'static str),
    /// Ascensions: `announcement_text`.
    pub announcement: &'static str,
}

/// The path's passive (`/datum/status_effect/heretic_passive`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PassiveDef {
    pub name: &'static str,
    pub recharge: &'static str,
    pub descriptions: &'static [&'static str],
}

/// One `/datum/heretic_knowledge_tree_column`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathDef {
    pub route: &'static str,
    pub ui_bgr: &'static str,
    pub complexity: &'static str,
    pub complexity_color: &'static str,
    pub icon: (&'static str, &'static str),
    pub description: &'static [&'static str],
    pub pros: &'static [&'static str],
    pub cons: &'static [&'static str],
    pub tips: &'static [&'static str],
    pub start: &'static str,
    pub tier1: &'static str,
    pub tier2: &'static str,
    pub robes: &'static str,
    pub tier3: &'static str,
    pub blade: &'static str,
    pub tier4: &'static str,
    pub ascension: &'static str,
    pub guaranteed: [&'static str; 3],
    pub shop_cost_discount: u32,
    pub passive: PassiveDef,
}

pub const BASIC: &str = "/datum/heretic_knowledge/spell/basic";
pub const LIVING_HEART: &str = "/datum/heretic_knowledge/living_heart";
pub const HUNT_AND_SACRIFICE: &str = "/datum/heretic_knowledge/hunt_and_sacrifice";
pub const FEAST_OF_OWLS: &str = "/datum/heretic_knowledge/feast_of_owls";
const CLOAK_OF_SHADOWS: &str = "/datum/heretic_knowledge/spell/cloak_of_shadows";
const RIFLE: &str = "/datum/heretic_knowledge/rifle";
const RIFLE_AMMO: &str = "/datum/heretic_knowledge/rifle_ammo";
/// `BGR_SIDE`.
pub const BGR_SIDE: &str = "node_side";
/// `HERETIC_DRAFT_TIER_MAX`.
const DRAFT_TIER_MAX: u32 = 5;

/// `HKT_DEPTH_*`.
pub const DEPTH_START: u32 = 2;
pub const DEPTH_TIER_1: u32 = 3;
pub const DEPTH_DRAFT_1: u32 = 4;
pub const DEPTH_TIER_2: u32 = 5;
pub const DEPTH_DRAFT_2: u32 = 6;
pub const DEPTH_ROBES: u32 = 7;
pub const DEPTH_TIER_3: u32 = 8;
pub const DEPTH_DRAFT_3: u32 = 9;
pub const DEPTH_ARMOR: u32 = 10;
pub const DEPTH_TIER_4: u32 = 11;
pub const DEPTH_DRAFT_4: u32 = 12;
pub const DEPTH_ASCENSION: u32 = 13;

pub fn def(path: &str) -> Option<&'static KnowledgeDef> {
    KNOWLEDGE.iter().find(|k| k.path == path)
}

pub fn path_def(route: &str) -> Option<&'static PathDef> {
    PATHS.iter().find(|p| p.route == route)
}

/// The path whose starting knowledge this is.
pub fn path_of_start(start: &str) -> Option<&'static PathDef> {
    PATHS.iter().find(|p| p.start == start)
}

/// `GLOB.heretic_start_knowledge`.
pub fn start_knowledge() -> impl Iterator<Item = &'static KnowledgeDef> {
    KNOWLEDGE.iter().filter(|k| k.starting)
}

/// A shop (`HERETIC_KNOWLEDGE_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Start,
    Tree,
    Shop,
    Draft,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Tree => "tree",
            Self::Shop => "shop",
            Self::Draft => "draft",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Start, Self::Tree, Self::Shop, Self::Draft]
            .into_iter()
            .find(|c| c.as_str() == s)
    }
}

/// `make_knowledge_id`: `"<category>/datum_heretic_knowledge_..."`.
pub fn make_id(path: &str, category: Category) -> String {
    format!(
        "{}/{}",
        category.as_str(),
        path.strip_prefix('/').unwrap_or(path).replace('/', "_")
    )
}

/// A shop entry (`make_knowledge_entry`).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: &'static str,
    pub next: Vec<String>,
    pub ban: Vec<String>,
    pub depth: u32,
    /// `HKT_PURCHASED_DEPTH`: the depth once researched (drafts), 0 = unchanged.
    pub purchased_depth: u32,
    pub bgr: &'static str,
    pub cost: u32,
    pub route: Option<&'static str>,
    pub category: Category,
    pub id: String,
}

impl Entry {
    pub fn new(
        path: &'static str,
        column: Option<&PathDef>,
        category: Category,
        depth: u32,
        cost: Option<u32>,
    ) -> Self {
        Self {
            path,
            next: Vec::new(),
            ban: Vec::new(),
            depth,
            purchased_depth: 0,
            bgr: column.map_or(BGR_SIDE, |c| c.ui_bgr),
            cost: cost.unwrap_or_else(|| def(path).map_or(0, |k| k.cost)),
            route: column.map(|c| c.route),
            category,
            id: make_id(path, category),
        }
    }

    pub fn def(&self) -> &'static KnowledgeDef {
        def(self.path).expect("entries name table knowledge")
    }
}

/// An ordered shop: `heretic_shops[category]`.
pub type Shop = Vec<Entry>;

fn entry_mut<'a>(shop: &'a mut Shop, path: &str) -> Option<&'a mut Entry> {
    shop.iter_mut().find(|e| e.path == path)
}

fn push_unique(list: &mut Vec<String>, id: String) {
    if !list.contains(&id) {
        list.push(id);
    }
}

/// `generate_heretic_starting_knowledge`: the start knowledge, then every path's start and tiers
/// (the tiers only for the path previews); the basic knowledge leads to every path's start, a
/// start to its tree's first tier, and each start bans the others.
pub fn start_shop() -> Shop {
    let mut shop: Shop = start_knowledge()
        .map(|k| Entry::new(k.path, None, Category::Start, 1, None))
        .collect();
    let mut starts = Vec::new();
    for column in PATHS {
        for (path, depth) in [
            (column.start, DEPTH_START),
            (column.tier1, DEPTH_TIER_1),
            (column.tier2, DEPTH_TIER_2),
            (column.tier3, DEPTH_TIER_3),
            (column.tier4, DEPTH_TIER_4),
        ] {
            shop.retain(|e| e.path != path);
            shop.push(Entry::new(path, Some(column), Category::Start, depth, None));
        }
        let start_id = make_id(column.start, Category::Start);
        if let Some(basic) = entry_mut(&mut shop, BASIC) {
            push_unique(&mut basic.next, start_id.clone());
        }
        if let Some(start) = entry_mut(&mut shop, column.start) {
            push_unique(&mut start.next, make_id(column.tier1, Category::Tree));
        }
        starts.push(start_id);
    }
    for entry in &mut shop {
        if starts.contains(&entry.id) {
            let own = entry.id.clone();
            for other in starts.iter().filter(|s| **s != own) {
                push_unique(&mut entry.ban, other.clone());
            }
        }
    }
    shop
}

/// `generate_heretic_path`: tier 1 → tier 2 → robes → tier 3 → blade → tier 4 → ascension.
pub fn path_tree(column: &PathDef) -> Shop {
    let order = [
        (column.tier1, DEPTH_TIER_1),
        (column.tier2, DEPTH_TIER_2),
        (column.tier3, DEPTH_TIER_3),
        (column.tier4, DEPTH_TIER_4),
        (column.robes, DEPTH_ROBES),
        (column.blade, DEPTH_ARMOR),
        (column.ascension, DEPTH_ASCENSION),
    ];
    let mut tree: Shop = order
        .iter()
        .map(|&(path, depth)| Entry::new(path, Some(column), Category::Tree, depth, None))
        .collect();
    let id = |path| make_id(path, Category::Tree);
    for (from, to) in [
        (column.tier1, column.tier2),
        (column.tier2, column.robes),
        (column.robes, column.tier3),
        (column.tier3, column.blade),
        (column.blade, column.tier4),
        (column.tier4, column.ascension),
    ] {
        if let Some(e) = entry_mut(&mut tree, from) {
            push_unique(&mut e.next, id(to));
        }
    }
    tree
}

/// `determine_drafted_knowledge`: three random side knowledges (the path's guaranteed one first)
/// after each of the four tiers, one of each draft researchable for free, and every side
/// knowledge in the shop by its drafting tier. `pick(n)` is a random index below `n`.
pub fn drafts(
    column: &PathDef,
    tree: &mut Shop,
    mut pick: impl FnMut(usize) -> usize,
) -> (Shop, Shop) {
    let discount = column.shop_cost_discount;
    let shop_costs: Vec<u32> = [1, 2, 2, 2, 3]
        .iter()
        .map(|c: &u32| c.saturating_sub(discount).max(1))
        .collect();
    let path_knowledge = [column.tier1, column.tier2, column.tier3, column.tier4];
    let ineligible: Vec<&str> = path_knowledge
        .iter()
        .chain(column.guaranteed.iter())
        .copied()
        .filter(|p| !p.is_empty())
        .collect();
    let shop_unlock_order = [
        column.tier1,
        column.tier2,
        column.robes,
        column.tier3,
        column.tier4,
    ];
    let mut eligible: Vec<Vec<&'static str>> = vec![Vec::new(); DRAFT_TIER_MAX as usize];
    let mut shop_knowledge: Vec<Vec<&'static str>> = vec![Vec::new(); DRAFT_TIER_MAX as usize];
    for k in KNOWLEDGE {
        if k.drafting_tier == 0 || k.drafting_tier > DRAFT_TIER_MAX {
            continue;
        }
        // `is_path_in_list`: the type or a subtype of an ineligible one.
        if ineligible
            .iter()
            .any(|p| k.path == *p || k.path.starts_with(&format!("{p}/")))
        {
            continue;
        }
        let tier = k.drafting_tier as usize - 1;
        if !k.shop_only {
            eligible[tier].push(k.path);
        }
        shop_knowledge[tier].push(k.path);
    }
    struct Draft {
        parent: &'static str,
        guaranteed: &'static str,
        supplementary: &'static [&'static str],
        probabilities: [u32; 5],
        depth: u32,
    }
    let drafts = [
        Draft {
            parent: column.tier1,
            guaranteed: column.guaranteed[0],
            supplementary: &[CLOAK_OF_SHADOWS],
            probabilities: [50, 50, 0, 0, 0],
            depth: DEPTH_DRAFT_1,
        },
        Draft {
            parent: column.tier2,
            guaranteed: column.guaranteed[1],
            supplementary: &[],
            probabilities: [50, 25, 25, 0, 0],
            depth: DEPTH_DRAFT_2,
        },
        Draft {
            parent: column.tier3,
            guaranteed: column.guaranteed[2],
            supplementary: &[],
            probabilities: [20, 20, 20, 20, 20],
            depth: DEPTH_DRAFT_3,
        },
        Draft {
            parent: column.tier4,
            guaranteed: "",
            supplementary: &[],
            probabilities: [0, 0, 0, 0, 100],
            depth: DEPTH_DRAFT_4,
        },
    ];
    let mut final_draft: Shop = Vec::new();
    for draft in drafts {
        let mut picked: Vec<String> = Vec::new();
        let mut add = |final_draft: &mut Shop, tree: &mut Shop, path: &'static str| {
            final_draft.retain(|e| e.path != path);
            let mut entry = Entry::new(path, None, Category::Draft, draft.depth, Some(0));
            entry.purchased_depth = def(path).map_or(0, |k| k.drafting_tier);
            let id = entry.id.clone();
            final_draft.push(entry);
            if let Some(parent) = entry_mut(tree, draft.parent) {
                push_unique(&mut parent.next, id.clone());
            }
            picked.push(id);
        };
        for &s in draft.supplementary {
            add(&mut final_draft, tree, s);
        }
        for cycle in 0..3 {
            let selected = if cycle == 0 && !draft.guaranteed.is_empty() {
                let tier = def(draft.guaranteed).map_or(0, |k| k.drafting_tier) as usize;
                if (1..=shop_knowledge.len()).contains(&tier)
                    && !shop_knowledge[tier - 1].contains(&draft.guaranteed)
                {
                    shop_knowledge[tier - 1].push(draft.guaranteed);
                }
                Some(draft.guaranteed)
            } else {
                // `pick_weight(probabilities)`, clamped to the tiers left; an emptied tier is cut.
                let total: u32 = draft.probabilities.iter().sum();
                let mut roll = pick(total as usize) as u32;
                let mut chosen = 0;
                for (i, w) in draft.probabilities.iter().enumerate() {
                    if roll < *w {
                        chosen = i;
                        break;
                    }
                    roll -= w;
                }
                let chosen = chosen.min(eligible.len().saturating_sub(1));
                let tier = eligible.get_mut(chosen);
                match tier {
                    Some(tier) if !tier.is_empty() => {
                        let i = pick(tier.len());
                        let path = tier.remove(i);
                        if tier.is_empty() {
                            eligible.remove(chosen);
                        }
                        Some(path)
                    }
                    _ => None,
                }
            };
            if let Some(path) = selected {
                add(&mut final_draft, tree, path);
            }
        }
        for id in &picked {
            if let Some(e) = final_draft.iter_mut().find(|e| e.id == *id) {
                for other in picked.iter().filter(|o| *o != id) {
                    push_unique(&mut e.ban, other.clone());
                }
            }
        }
    }
    let mut shop: Shop = Vec::new();
    for (tier, paths) in shop_knowledge.iter().enumerate() {
        let unlocked_by = shop_unlock_order[tier];
        for &path in paths {
            shop.retain(|e| e.path != path);
            let entry = Entry::new(
                path,
                None,
                Category::Shop,
                tier as u32 + 1,
                Some(shop_costs[tier]),
            );
            let id = entry.id.clone();
            shop.push(entry);
            if let Some(parent) = entry_mut(tree, unlocked_by) {
                push_unique(&mut parent.next, id.clone());
            }
            if let Some(found) = entry_mut(&mut final_draft, path) {
                push_unique(&mut found.ban, id);
            }
        }
    }
    if def(RIFLE_AMMO).is_some() {
        shop.retain(|e| e.path != RIFLE_AMMO);
        let ammo = Entry::new(RIFLE_AMMO, None, Category::Shop, 2, None);
        let ammo_id = ammo.id.clone();
        shop.push(ammo);
        if let Some(gun) = entry_mut(&mut shop, RIFLE) {
            push_unique(&mut gun.next, ammo_id.clone());
        }
        if let Some(gun) = entry_mut(&mut final_draft, RIFLE) {
            push_unique(&mut gun.next, ammo_id);
        }
    }
    (shop, final_draft)
}

/// `get_researchable_knowledge`: every `next` of what is researched, minus their bans.
pub fn researchable<'a>(researched: impl Iterator<Item = &'a Entry>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut banned: Vec<String> = Vec::new();
    for e in researched {
        for n in &e.next {
            push_unique(&mut out, n.clone());
        }
        for b in &e.ban {
            push_unique(&mut banned, b.clone());
        }
    }
    out.retain(|id| !banned.contains(id));
    out
}

/// `/datum/objective/heretic_research` target: one point you spawn with, the path's tree, the
/// start knowledge, a third of the drafts (4 without any) and `rand(2, 4)`.
pub fn research_target(tree_len: usize, draft_len: usize, extra: u32) -> u32 {
    let tree = if tree_len == 0 { 10 } else { tree_len as u32 };
    let drafts = if draft_len == 0 {
        4
    } else {
        draft_len.div_ceil(3) as u32
    };
    1 + tree + start_knowledge().count() as u32 + drafts + extra
}

/// An atom near the rune, for recipe matching.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate<T> {
    pub thing: T,
    pub path: String,
    /// Stacks: the amount.
    pub stack: Option<u32>,
}

/// What a matched recipe uses.
#[derive(Clone, Debug, PartialEq)]
pub struct Matched<T> {
    /// Whole atoms.
    pub atoms: Vec<T>,
    /// Stack takes: (stack, amount).
    pub stacks: Vec<(T, u32)>,
}

pub fn is_type(path: &str, root: &str) -> bool {
    path == root || path.strip_prefix(root).is_some_and(|r| r.starts_with('/'))
}

/// `do_ritual`'s matching: each atom fills every requirement it is a type of that is still
/// wanted (a stack by its amount). `Err` lists the requirements still missing (index, amount).
pub fn match_recipe<T: Clone + PartialEq>(
    required: &[(&[&str], u32)],
    candidates: &[Candidate<T>],
) -> Result<Matched<T>, Vec<(usize, u32)>> {
    let mut left: Vec<u32> = required.iter().map(|(_, n)| *n).collect();
    let mut atoms: Vec<T> = Vec::new();
    let mut stack_reqs: Vec<Option<u32>> = vec![None; required.len()];
    for c in candidates {
        for (i, (types, _)) in required.iter().enumerate() {
            if left[i] == 0 || !types.iter().any(|t| is_type(&c.path, t)) {
                continue;
            }
            match c.stack {
                Some(amount) => {
                    stack_reqs[i].get_or_insert(left[i]);
                    left[i] -= amount.min(left[i]);
                }
                None => {
                    left[i] -= 1;
                    if !atoms.contains(&c.thing) {
                        atoms.push(c.thing.clone());
                    }
                }
            }
        }
    }
    let missing: Vec<(usize, u32)> = left
        .iter()
        .enumerate()
        .filter(|(_, n)| **n > 0)
        .map(|(i, n)| (i, *n))
        .collect();
    if !missing.is_empty() {
        return Err(missing);
    }
    // The wanted amounts out of the stacks, in order.
    let mut stacks: Vec<(T, u32)> = Vec::new();
    for (i, (types, _)) in required.iter().enumerate() {
        let Some(mut want) = stack_reqs[i] else {
            continue;
        };
        for c in candidates {
            let Some(amount) = c.stack else {
                continue;
            };
            if want == 0 {
                break;
            }
            if !types.iter().any(|t| is_type(&c.path, t)) {
                continue;
            }
            let taken: u32 = stacks
                .iter()
                .filter(|(t, _)| *t == c.thing)
                .map(|(_, n)| n)
                .sum();
            let take = amount.saturating_sub(taken).min(want);
            if take > 0 {
                stacks.push((c.thing.clone(), take));
                want -= take;
            }
        }
    }
    Ok(Matched { atoms, stacks })
}

/// `parse_required_item`: "тело"/"тела" for humans, any mob, else the type's name.
pub fn describe_required(path: &str, amount: u32, name: impl Fn(&str) -> String) -> String {
    if is_type(path, "/mob/living/carbon/human") {
        return if amount > 1 { "тела" } else { "тело" }.to_owned();
    }
    if is_type(path, "/mob/living") {
        return if amount > 1 {
            "каркасы любого типа"
        } else {
            "каркас любого типа"
        }
        .to_owned();
    }
    let n = name(path);
    if amount > 1 {
        format!("{amount} {n}")
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ash() -> &'static PathDef {
        path_def("Путь Пепла").unwrap()
    }

    #[test]
    fn ids_follow_tg() {
        assert_eq!(
            make_id(BASIC, Category::Tree),
            "tree/datum_heretic_knowledge_spell_basic"
        );
        assert_eq!(Category::parse("draft"), Some(Category::Draft));
        assert_eq!(Category::parse("nope"), None);
    }

    #[test]
    fn table_has_the_start_knowledge_and_eight_paths() {
        let starts: Vec<&str> = start_knowledge().map(|k| k.name).collect();
        assert_eq!(starts.len(), 4);
        for name in [
            "Рассвет",
            "Живое сердце",
            "Сердцебиение Мансуса",
            "Пир для Сов",
        ] {
            assert!(starts.contains(&name), "{name} in {starts:?}");
        }
        assert_eq!(PATHS.len(), 8);
        let ash = ash();
        assert_eq!(
            ash.start,
            "/datum/heretic_knowledge/limited_amount/starting/base_ash"
        );
        let start = def(ash.start).unwrap();
        assert_eq!(
            (start.kind, start.cost, start.limit),
            (Kind::Starting, 1, 2)
        );
        assert_eq!(start.results, ["/obj/item/melee/sickly_blade/ash"]);
        assert_eq!(def(BASIC).unwrap().max_charges, 8);
        assert_eq!(def(ash.ascension).unwrap().kind, Kind::Ultimate);
        assert_eq!(def(ash.ascension).unwrap().cost, 2);
    }

    #[test]
    fn the_basic_knowledge_leads_to_one_path_only() {
        let shop = start_shop();
        let basic = shop.iter().find(|e| e.path == BASIC).unwrap();
        assert_eq!(basic.next.len(), 8);
        let ash = shop.iter().find(|e| e.path == ash().start).unwrap();
        assert_eq!(ash.depth, DEPTH_START);
        assert_eq!(ash.ban.len(), 7);
        assert_eq!(ash.next, [make_id(ash_def().tier1, Category::Tree)]);
        // Researched: the start knowledge. Researchable: the eight starts.
        let researched: Vec<Entry> = shop
            .iter()
            .filter(|e| def(e.path).unwrap().starting)
            .cloned()
            .collect();
        let can = researchable(researched.iter());
        assert_eq!(can.len(), 8);
        assert!(can.contains(&ash.id));
        // With the ash start, the other starts are banned and its tree's first tier is next
        // (the start itself stays listed: tg drops researched entries from their shop instead).
        let mut with_ash = researched.clone();
        with_ash.push(ash.clone());
        let can = researchable(with_ash.iter());
        assert_eq!(
            can,
            [ash.id.clone(), make_id(ash_def().tier1, Category::Tree)]
        );
    }

    fn ash_def() -> &'static PathDef {
        ash()
    }

    #[test]
    fn a_path_tree_unlocks_in_order() {
        let column = ash();
        let tree = path_tree(column);
        assert_eq!(tree.len(), 7);
        let next_of = |p: &str| tree.iter().find(|e| e.path == p).unwrap().next.clone();
        assert_eq!(
            next_of(column.tier1),
            [make_id(column.tier2, Category::Tree)]
        );
        assert_eq!(
            next_of(column.tier2),
            [make_id(column.robes, Category::Tree)]
        );
        assert_eq!(
            next_of(column.robes),
            [make_id(column.tier3, Category::Tree)]
        );
        assert_eq!(
            next_of(column.tier4),
            [make_id(column.ascension, Category::Tree)]
        );
        let asc = tree.iter().find(|e| e.path == column.ascension).unwrap();
        assert_eq!(
            (asc.depth, asc.cost, asc.bgr),
            (DEPTH_ASCENSION, 2, "node_ash")
        );
    }

    #[test]
    fn drafts_offer_three_per_tier_and_the_shop_sells_the_rest() {
        let column = ash();
        let mut tree = path_tree(column);
        let mut n = 0usize;
        let (shop, draft) = drafts(column, &mut tree, |len| {
            n += 7;
            n % len
        });
        // Cloak of shadows plus three per tier: 13; the guaranteed ones are there.
        assert_eq!(draft.len(), 13);
        for g in column.guaranteed {
            assert!(draft.iter().any(|e| e.path == g), "{g} drafted");
        }
        assert!(draft.iter().all(|e| e.cost == 0));
        // A draft bans its siblings: researching one leaves the other two.
        let first = draft
            .iter()
            .find(|e| e.path == column.guaranteed[0])
            .unwrap();
        assert!(first.ban.len() >= 3);
        // The tier-1 shop opens with the path's first tier, costing 1.
        let t1 = tree.iter().find(|e| e.path == column.tier1).unwrap();
        let medallion = shop
            .iter()
            .find(|e| e.path == column.guaranteed[0])
            .unwrap();
        assert_eq!(medallion.cost, 1);
        assert!(t1.next.contains(&medallion.id));
        // tg's ammo entry keeps the knowledge's own cost (0).
        assert!(shop.iter().all(|e| e.cost >= 1 || e.path == RIFLE_AMMO));
        assert!(shop.iter().any(|e| e.path == RIFLE_AMMO));
        // Path knowledge is never drafted nor sold.
        for p in [column.tier1, column.tier2, column.tier3, column.tier4] {
            assert!(!draft.iter().any(|e| e.path == p));
            assert!(!shop.iter().any(|e| e.path == p));
        }
        assert_eq!(
            research_target(tree.len(), draft.len(), 2),
            1 + 7 + 4 + 5 + 2
        );
        assert_eq!(research_target(0, 0, 3), 1 + 10 + 4 + 4 + 3);
    }

    #[test]
    fn recipes_match_by_type_and_stack_amount() {
        let blade: &[(&[&str], u32)] = &[(&["/obj/item/knife"], 1), (&["/obj/item/match"], 1)];
        let c = |thing: u32, path: &str, stack: Option<u32>| Candidate {
            thing,
            path: path.to_owned(),
            stack,
        };
        let near = [
            c(1, "/obj/item/knife/kitchen", None),
            c(2, "/obj/item/match", None),
            c(3, "/obj/item/knifes", None),
        ];
        let m = match_recipe(blade, &near).unwrap();
        assert_eq!(m.atoms, [1, 2]);
        assert_eq!(match_recipe(blade, &near[..1]), Err(vec![(1, 1)]));
        // Any of several types; a stack fills by its amount.
        let armor: &[(&[&str], u32)] = &[
            (&["/obj/structure/table", "/obj/item/clothing/suit"], 1),
            (&["/obj/item/stack/sheet/iron"], 5),
        ];
        let near = [
            c(4, "/obj/item/clothing/suit/apron", None),
            c(5, "/obj/item/stack/sheet/iron", Some(3)),
            c(6, "/obj/item/stack/sheet/iron", Some(10)),
        ];
        let m = match_recipe(armor, &near).unwrap();
        assert_eq!(m.atoms, [4]);
        assert_eq!(m.stacks, [(5, 3), (6, 2)]);
        assert_eq!(match_recipe(armor, &near[..2]), Err(vec![(1, 2)]));
        // The ascension wants three bodies.
        let asc = def(ash().ascension).unwrap().required;
        let bodies: Vec<_> = (0..3)
            .map(|i| c(10 + i, "/mob/living/carbon/human", None))
            .collect();
        assert_eq!(match_recipe(asc, &bodies).unwrap().atoms.len(), 3);
        assert_eq!(match_recipe(asc, &bodies[..2]), Err(vec![(0, 1)]));
        assert_eq!(
            describe_required("/mob/living/carbon/human", 3, |_| String::new()),
            "тела"
        );
    }
}
