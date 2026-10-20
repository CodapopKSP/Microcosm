//! Species and genera: founding, naming, population tracking, phylogeny.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::genome::{Genome, drive, gene};
use crate::parts::{self, PartKind};
use crate::rng::{Rng, mix64};
use crate::traits::{self, Traits};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Species {
    pub id: u32,
    pub name: String,
    pub genus: u32,
    pub parent: Option<u32>,
    pub founder: Genome,
    pub founded: u64,
    pub extinct: Option<u64>,
    pub population: u32,
    pub peak: u32,
    /// Named and listed once it reaches `species.established` members.
    pub established: bool,
    pub established_at: Option<u64>,
    pub color: [f32; 3],
    pub total_born: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Genus {
    pub id: u32,
    pub name: String,
    pub founder: Genome,
    /// Diet root of the name, passed on to genera that branch off and eat the same way.
    #[serde(default)]
    pub root: String,
}

/// What happened to species this tick, for the event log.
#[derive(Clone, Debug)]
pub enum SpeciesEvent {
    Established(u32),
    Extinct(u32),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeciesRegistry {
    pub species: BTreeMap<u32, Species>,
    pub genera: BTreeMap<u32, Genus>,
    next_species: u32,
    next_genus: u32,
    #[serde(skip)]
    pub events: Vec<SpeciesEvent>,
}

// Names are built from what an organism is like, so a name tells a viewer something:
//
// - Genus: prefix + diet root + size ending, e.g. "Acantho" + "lith" + "ella". The prefix is the
//   body's most striking feature (spines, tentacles, eyes, size...) or else its colour. A genus
//   that branches off keeps its parent's diet root while it eats the same way, so related genera
//   sound related ("Chlorophycus" → "Cirrophycella").
// - Species epithet: for founders, a "first of its line" word. Otherwise whatever sets the species
//   apart from the named species it split from (gained eyes: "oculatus"; lost them: "caecus"),
//   weighed a little by what it is outright. Features the genus name already says are avoided.
//   Then colour and pattern words, then neutral ones, then roman numerals.

/// Hue buckets: genus prefix, centre hue in degrees, and matching epithets.
const COLORS: [(&str, f32, &[&str]); 8] = [
    ("Rubro", 0.0, &["ruber", "rubens", "sanguineus", "coccineus"]),
    ("Auranti", 30.0, &["aurantiacus", "fulvus", "croceus"]),
    ("Xantho", 55.0, &["flavus", "luteus", "aureus", "sulphureus"]),
    ("Chloro", 110.0, &["viridis", "virens", "prasinus", "smaragdinus"]),
    ("Thalasso", 165.0, &["thalassinus", "aeruginosus", "glaucescens"]),
    ("Cyano", 200.0, &["caeruleus", "cyaneus", "azureus"]),
    ("Ianthino", 255.0, &["violaceus", "ianthinus", "purpureus"]),
    ("Rhodo", 315.0, &["roseus", "rhodinus", "carneus"]),
];
const PALE: (&[&str], &[&str]) = (
    &["Leuco", "Glauco"],
    &["albus", "candidus", "griseus", "pallens", "hyalinus"],
);
const DARK: (&[&str], &[&str]) = (&["Melano", "Phaeo"], &["niger", "ater", "fuscus"]);
const PATTERNS: [&[&str]; 4] = [
    &[],
    &["maculatus", "guttatus", "punctatus"],
    &["striatus", "vittatus", "lineatus"],
    &["annulatus", "cinctus", "zonatus"],
];

/// Colour prefixes and colour/pattern epithets for a body.
fn color_words(t: &Traits) -> (&'static [&'static str], Vec<&'static str>) {
    let [r, g, b] = t.color;
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let (prefixes, colors): (&[&str], &[&str]) = if luminance < 0.1 {
        DARK
    } else if t.saturation < 50 {
        PALE
    } else {
        let hue = t.hue as f32 / 256.0 * 360.0;
        let dist = |c: f32| (hue - c).abs().min(360.0 - (hue - c).abs());
        let c = COLORS
            .iter()
            .min_by(|a, b| dist(a.1).partial_cmp(&dist(b.1)).unwrap())
            .unwrap();
        (std::slice::from_ref(&c.0), c.2)
    };
    let mut words = colors.to_vec();
    words.extend_from_slice(PATTERNS[(t.pattern >> 6) as usize]);
    (prefixes, words)
}

/// Diet roots for genus names (photosynthesis, chemosynthesis, scavenging, hunting).
const ROOTS: [&[&str]; 4] = [
    &[
        "phyta", "coccus", "phycus", "chloris", "plasma", "monas", "sphaera", "cystis", "actis",
        "phyllum", "thallus", "helion",
    ],
    &[
        "lithus", "thermus", "sulfa", "ferrum", "chemon", "thion", "siderus", "petra", "fumaria",
        "bacter", "chalcis", "pyrion",
    ],
    &[
        "phagus", "sapros", "necrus", "limax", "detrix", "taphus", "thanatus", "pelos", "ilys",
        "coprus", "lysis", "boros",
    ],
    &[
        "vorax", "odon", "raptor", "lamia", "phagon", "dracon", "lycus", "harpax", "echis",
        "therium", "sphyra", "cetus",
    ],
];

/// Genus name endings by adult size.
const ENDINGS: [&[&str]; 3] = [
    &["", "ella", "ula", "ina", "idium", "iscus"],
    &["", "is", "ops", "ella", "ia", "ides"],
    &["", "ops", "us", "on", "ia", "otherium"],
];

/// Something an organism can be described by: epithets (synonyms), the genus prefixes that say
/// the same thing, and how strongly it applies (> 0.25 is distinctive).
struct Feature {
    epithets: &'static [&'static str],
    prefixes: &'static [&'static str],
    score: f32,
}

/// Every feature, always in the same order (so two bodies can be compared feature by feature).
fn features(t: &Traits, genome: &Genome) -> Vec<Feature> {
    let d = &t.drives;
    // How much of each part kind the body carries (copies × size).
    let mut amount = [0.0f32; PartKind::ALL.len()];
    let segments = t.segments as f32;
    for p in parts::decode_parts(genome) {
        let repeats = if p.anchor == parts::Anchor::EverySegment {
            segments
        } else {
            1.0
        };
        amount[p.kind as usize - 1] += p.count as f32 * repeats * p.size;
    }
    let part = |k: PartKind| 1.0 - (-1.5 * amount[k as usize - 1]).exp();
    let mut diets = [t.photo, t.chemo, t.scavenge, t.hunt];
    diets.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let mixed = |n: usize| {
        if diets[0] > 0.1 {
            diets[n] / diets[0]
        } else {
            0.0
        }
    };
    let f = |epithets, prefixes, score| Feature {
        epithets,
        prefixes,
        score,
    };
    vec![
        // Size and shape.
        f(
            &["magnus", "grandis", "major"],
            &["Macro"],
            (t.adult_radius / 24.0).min(2.0) - 0.4,
        ),
        f(
            &["gigas", "giganteus", "colossus"],
            &["Mega"],
            (t.adult_radius / 60.0).min(2.0) - 0.5,
        ),
        f(
            &["minimus", "minutus", "pusillus", "nanus"],
            &["Micro", "Nano"],
            1.0 - t.adult_radius / 3.0,
        ),
        f(
            &["elongatus", "vermiformis", "longissimus"],
            &["Nemato", "Ophio"],
            t.elongation * 1.4 - 0.3,
        ),
        f(
            &["globosus", "rotundatus"],
            &[],
            (0.1 - t.elongation) * 2.0,
        ),
        f(
            &["segmentatus", "articulatus", "annelidus"],
            &["Arthro"],
            (t.segments as f32 - 2.0) / 3.0,
        ),
        f(
            &["complexus", "polymorphus", "multiformis"],
            &["Poly"],
            (t.part_count as f32 - 6.0) / 6.0,
        ),
        f(&["nudus", "glaber"], &[], -(t.part_count as f32) / 4.0),
        // Movement.
        f(
            &["velox", "celer", "rapidus"],
            &["Tachy"],
            t.swim * 1.5 - 0.3,
        ),
        f(&["agilis", "versutus"], &[], t.turn_rate / 4.0 - 0.3),
        f(
            &["ambulans", "gradiens", "cursor"],
            &[],
            t.crawl_coeff * 1.2 - 0.2,
        ),
        f(
            &["vagans", "errabundus", "inquietus"],
            &[],
            t.restlessness * 1.2 - 0.2,
        ),
        f(
            &["sedentarius", "otiosus"],
            &[],
            (0.1 - t.restlessness) * 2.0,
        ),
        f(
            &["natans", "fluitans", "pelagicus"],
            &["Pelago"],
            genome.signed(gene::BUOYANCY) * 1.5 - 0.3,
        ),
        f(
            &["mergens", "demersus", "benthicus"],
            &["Bentho"],
            -genome.signed(gene::BUOYANCY) * 1.2,
        ),
        // Feeding.
        f(
            &["ferox", "saevus", "truculentus"],
            &[],
            t.hunt * 1.2 + t.jaws * 0.5 - 0.2,
        ),
        f(
            &["venator", "praedator", "persecutor"],
            &[],
            t.hunt + t.swim * 0.8 - 0.5,
        ),
        f(
            &["insidiator", "insidiosus", "latens"],
            &[],
            t.lure + t.hunt * 0.8 + (1.0 - t.restlessness) * 0.3 - 0.6,
        ),
        f(
            &["necrophagus", "vespillo", "pollinctor"],
            &[],
            t.scavenge * 1.3 - 0.3,
        ),
        f(
            &["lithophagus", "chemotrophus"],
            &[],
            t.chemo * 1.1 - 0.3,
        ),
        f(
            &["mixotrophus", "ambiguus", "duplex"],
            &["Mixo"],
            mixed(1) * 1.5 - 0.6,
        ),
        f(
            &["omnivorus", "pantophagus"],
            &["Panto"],
            mixed(2) * 1.6 - 0.5,
        ),
        f(
            &["filtrator", "colans", "cribrarius"],
            &[],
            t.filter * 0.8 - 0.1,
        ),
        // Parts.
        f(
            &["flagellatus", "mastigophorus"],
            &["Mastigo"],
            part(PartKind::Flagellum) * 1.2 - 0.2,
        ),
        f(
            &["ciliatus", "crinitus", "pilosus"],
            &["Tricho"],
            part(PartKind::Cilia) * 1.2 - 0.2,
        ),
        f(
            &["tentaculatus", "cirratus", "polypus", "brachiatus"],
            &["Cirro", "Plecto"],
            part(PartKind::Tentacle) * 1.4 - 0.1,
        ),
        f(
            &["frondosus", "foliaceus", "phylloides"],
            &["Phyllo"],
            part(PartKind::Frond) * 1.4 - 0.1,
        ),
        f(
            &["pinnatus", "alatus", "pterygius"],
            &["Ptero"],
            part(PartKind::Fin) * 1.3 - 0.1,
        ),
        f(
            &["caudatus", "longicaudatus"],
            &["Uro"],
            part(PartKind::Fluke) * 1.3 - 0.1,
        ),
        f(
            &["pedatus", "multipes", "gressorius"],
            &["Podo"],
            part(PartKind::Leg) * 1.4 - 0.1,
        ),
        f(
            &["affixus", "radicatus", "adhaerens"],
            &["Rhizo"],
            part(PartKind::Holdfast) * 1.3 - 0.1,
        ),
        f(
            &["stipitatus", "pedicellatus", "caulescens"],
            &["Caulo"],
            part(PartKind::Stalk) * 1.2 - 0.1,
        ),
        f(
            &["oculatus", "videns", "perspicax"],
            &["Ommato"],
            part(PartKind::Eye) * 1.3 - 0.1,
        ),
        f(
            &["argus", "multioculatus", "polyophthalmus"],
            &[],
            (t.eye_count as f32 - 2.0) / 3.0,
        ),
        f(
            &["caecus", "anophthalmus"],
            &[],
            -part(PartKind::Eye) * 1.3,
        ),
        f(
            &["antennatus", "cornutus", "palpatus"],
            &["Cerato"],
            part(PartKind::Antenna) * 1.3 - 0.1,
        ),
        f(
            &["sagax", "olfactor"],
            &[],
            (t.sense_radii - 4.0) / 12.0 - 0.2,
        ),
        f(
            &["lucifer", "lampadarius", "noctiluca", "lucens"],
            &["Lampro", "Photo"],
            part(PartKind::Lure) * 1.6,
        ),
        f(
            &["spinosus", "aculeatus", "echinatus", "horridus"],
            &["Acantho", "Echino"],
            part(PartKind::Spine) * 1.4,
        ),
        f(
            &["dentatus", "mandibulatus", "mordax"],
            &["Gnatho"],
            part(PartKind::Jaw) * 1.3,
        ),
        f(
            &["loricatus", "testaceus", "cataphractus", "conchatus"],
            &["Ostraco", "Concho"],
            part(PartKind::Shell) * 1.4,
        ),
        f(
            &["inermis", "indefensus"],
            &[],
            -(t.armor + t.spikes + t.toxin) * 1.5,
        ),
        // Chemistry.
        f(
            &["toxicus", "venenatus", "virulentus", "noxius"],
            &["Toxo"],
            t.toxin * 1.4 - 0.1,
        ),
        f(
            &["immunis", "resistens"],
            &[],
            t.resistance * 1.3 - 0.4,
        ),
        f(
            &["anaerobius", "anoxicus"],
            &[],
            (0.35 - t.aerobic) * 3.0,
        ),
        f(
            &["azotifer", "nitrogenius"],
            &[],
            t.fixation * 1.3 - 0.2,
        ),
        f(
            &["sulfureus", "sulfurarius"],
            &["Thio"],
            (0.5 - t.oxygenic) * 2.0 * t.photo,
        ),
        f(
            &["bibulus", "sorbens"],
            &[],
            t.uptake * 1.2 - 0.4,
        ),
        // Temperature.
        f(
            &["thermophilus", "calidus", "fervidus"],
            &["Pyro", "Thermo"],
            (t.temp_pref - 26.0) / 10.0,
        ),
        f(
            &["frigidus", "glacialis", "psychrophilus"],
            &["Cryo"],
            (10.0 - t.temp_pref) / 8.0,
        ),
        f(
            &["eurythermus", "tolerans"],
            &[],
            genome.frac(gene::TEMP_TOL) * 1.2 - 0.4,
        ),
        f(
            &["stenothermus", "delicatulus"],
            &[],
            (0.15 - genome.frac(gene::TEMP_TOL)) * 2.0,
        ),
        // Drives.
        f(&["photophilus", "heliotropus"], &[], d[drive::LIGHT]),
        f(
            &["lucifugus", "umbraticus", "tenebrosus"],
            &["Scoto"],
            -d[drive::LIGHT],
        ),
        f(&["tepidarius", "apricans"], &[], d[drive::WARMTH] * 0.9),
        f(
            &["mineralis", "metallophilus"],
            &[],
            d[drive::MINERALS] * 0.9,
        ),
        f(
            &["necrophilus", "cadaverinus"],
            &[],
            d[drive::CARRION] * 0.9,
        ),
        f(
            &["persequens", "sectator"],
            &[],
            d[drive::SMALLER] * (0.4 + t.hunt),
        ),
        f(
            &["timidus", "pavidus", "fugiens"],
            &[],
            -d[drive::LARGER] * 1.1,
        ),
        f(
            &["audax", "intrepidus", "temerarius"],
            &[],
            d[drive::LARGER] * 1.1,
        ),
        f(
            &["gregarius", "socialis", "consors"],
            &[],
            d[drive::KIN] * 1.2,
        ),
        f(
            &["solitarius", "eremita", "singularius"],
            &[],
            -d[drive::KIN] * 1.2,
        ),
        f(
            &["congregans", "confertus", "turbaris"],
            &[],
            d[drive::CROWDING] * 1.1,
        ),
        f(
            &["dispersus", "sparsus", "secretarius"],
            &[],
            -d[drive::CROWDING] * 1.1,
        ),
        f(&["aerophilus", "oxyphilus"], &[], d[drive::OXYGEN] * 0.9),
        f(&["aerophobus", "oxyphobus"], &[], -d[drive::OXYGEN] * 0.9),
        // Life history.
        f(
            &["longaevus", "perennis", "annosus"],
            &[],
            genome.frac(gene::LONGEVITY) * 1.2 - 0.2,
        ),
        f(
            &["fugax", "ephemerus", "brevis"],
            &[],
            0.3 - genome.frac(gene::LONGEVITY) * 3.0,
        ),
        f(
            &["prolificus", "fecundus", "multiparus"],
            &[],
            (t.brood as f32 - 2.0) / 3.0,
        ),
        f(
            &["patiens", "cunctator"],
            &[],
            genome.frac(gene::PATIENCE) * 1.2 - 0.5,
        ),
        f(
            &["properans", "festinans"],
            &[],
            (0.15 - genome.frac(gene::PATIENCE)) * 2.0,
        ),
        f(
            &["parcus", "frugalis"],
            &[],
            genome.frac(gene::RESERVES) * 1.2 - 0.4,
        ),
        f(
            &["robustus", "validus", "firmus"],
            &[],
            genome.frac(gene::TOUGHNESS) * 1.2 - 0.2,
        ),
        f(
            &["tenax", "reviviscens"],
            &[],
            genome.frac(gene::REGEN) * 1.2 - 0.2,
        ),
    ]
}

/// The first species of a founding lineage.
const FOUNDER: [&str; 4] = ["primigenius", "primordialis", "pristinus", "archaeus"];

/// Neutral epithets used when nothing is distinctive (or distinctive ones are taken).
const PLAIN: [&str; 100] = [
    "vulgaris", "communis", "simplex", "modestus", "placidus", "errans", "tenuis", "novus",
    "obscurus", "gracilis", "lentus", "mollis", "tardus", "levis", "serenus", "clarus",
    "fidelis", "humilis", "pulcher", "rarus", "sollers", "tacitus", "varius", "vividus",
    "comptus", "delicatus", "hilaris", "insignis", "laetus", "mirabilis", "nitidus", "ornatus",
    "quietus", "splendens", "subtilis", "tranquillus", "venustus", "zephyrus", "amabilis",
    "aequalis", "bellus", "benignus", "blandus", "carus", "castus", "celatus", "civilis",
    "concinnus", "constans", "curiosus", "decorus", "dubius", "egregius", "elegans", "eximius",
    "facilis", "felix", "festivus", "formosus", "fortuitus", "generosus", "gentilis", "honestus",
    "ignotus", "incertus", "ingenuus", "iucundus", "lepidus", "liber", "memorabilis", "mitis",
    "mutabilis", "notabilis", "occultus", "pacatus", "parilis", "peregrinus", "perplexus",
    "pius", "probus", "prudens", "pudicus", "recens", "secretus", "similis", "singularis",
    "sobrius", "solitus", "spectabilis", "stabilis", "suavis", "tener", "urbanus",
    "verecundus", "verus", "vigilans", "amoenus", "callidus", "dulcis", "modicus",
];

fn shuffle<T>(rng: &mut Rng, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        items.swap(i, rng.below(i as u32 + 1) as usize);
    }
}

/// Join a genus prefix and a root ("Ostraco" + "odon" → "Ostracodon").
fn join_prefix(prefix: &str, root: &str) -> String {
    let vowel = |c: Option<char>| c.is_some_and(|c| "aeiouy".contains(c));
    if vowel(prefix.chars().last()) && vowel(root.chars().next()) {
        format!("{}{root}", &prefix[..prefix.len() - 1])
    } else {
        format!("{prefix}{root}")
    }
}

/// Add an ending, dropping the stem's own Latin ending first ("phycus" + "ella" → "phycella").
fn join_ending(stem: &str, ending: &str) -> String {
    if ending.is_empty() || stem.ends_with(ending) {
        return stem.to_string();
    }
    let mut base = stem;
    for e in ["us", "is", "os", "as", "um", "ys", "on", "a", "e", "i", "o", "u", "y"] {
        if let Some(b) = stem.strip_suffix(e) {
            // Keep enough of the root to recognise it.
            if b.len() >= 5 {
                base = b;
            }
            break;
        }
    }
    let vowel = |c: Option<char>| c.is_some_and(|c| "aeiouy".contains(c));
    if vowel(base.chars().last()) && vowel(ending.chars().next()) {
        base = &base[..base.len() - 1];
    }
    format!("{base}{ending}")
}

/// A genus name and the diet root it uses. `inherited` is the root of the genus it branched
/// from, if it still eats the same way.
fn genus_name(t: &Traits, genome: &Genome, inherited: Option<&str>, seed: u64) -> (String, String) {
    let mut rng = Rng::new(seed);
    // Prefix: the most striking body feature, unless the colour says as much.
    let (color, _) = color_words(t);
    let mut prefix = color[rng.below(color.len() as u32) as usize];
    let mut best = 0.6 + rng.f32() * 0.35;
    for feat in features(t, genome) {
        if feat.prefixes.is_empty() || feat.score < 0.6 {
            continue;
        }
        let score = feat.score + rng.f32() * 0.35;
        if score > best {
            best = score;
            prefix = feat.prefixes[rng.below(feat.prefixes.len() as u32) as usize];
        }
    }
    // Don't say the same thing twice ("Pyro" + "pyrion", "Chloro" + "chloris").
    let repeats = |root: &str| root.starts_with(&prefix.trim_end_matches('o').to_lowercase());
    let root = match inherited {
        Some(r) => r.to_string(),
        None => {
            let roots: Vec<&str> = ROOTS[t.main_diet()]
                .iter()
                .copied()
                .filter(|r| !repeats(r))
                .collect();
            roots[rng.below(roots.len() as u32) as usize].to_string()
        }
    };
    let endings = ENDINGS[if t.adult_radius < 4.0 {
        0
    } else if t.adult_radius < 24.0 {
        1
    } else {
        2
    }];
    let mut ending = endings[rng.below(endings.len() as u32) as usize];
    let stem = if repeats(&root) {
        // An inherited root that the prefix would repeat: the root alone.
        let mut chars = root.chars();
        let first = chars.next().unwrap().to_ascii_uppercase();
        format!("{first}{}", chars.as_str())
    } else {
        join_prefix(prefix, &root)
    };
    if stem.len() >= 13 {
        // Long enough already.
        ending = "";
    }
    (join_ending(&stem, ending), root)
}

/// Candidate epithets in order of preference. `parent` is the named species it split from;
/// `genus` is its genus name.
fn epithet_candidates(
    t: &Traits,
    genome: &Genome,
    parent: Option<(&Traits, &Genome)>,
    genus: &str,
    seed: u64,
) -> Vec<&'static str> {
    let mut rng = Rng::new(seed);
    let own = features(t, genome);
    let before = parent.map(|(pt, pg)| features(pt, pg));
    let mut scored: Vec<(&'static str, f32)> = Vec::new();
    for (i, feat) in own.iter().enumerate() {
        let mut score = match &before {
            // What changed since the parent matters most; what it is, a little.
            Some(b) => 0.4 * feat.score + 1.2 * (feat.score - b[i].score),
            None => feat.score,
        };
        if feat.prefixes.iter().any(|p| genus.starts_with(p)) {
            score *= 0.3;
        }
        let mut words = feat.epithets.to_vec();
        shuffle(&mut rng, &mut words);
        for (k, w) in words.into_iter().enumerate() {
            scored.push((w, score - 0.2 * k as f32));
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let mut out: Vec<&'static str> = Vec::new();
    if parent.is_none() {
        let mut founder = FOUNDER.to_vec();
        shuffle(&mut rng, &mut founder);
        out.extend(founder);
    }
    // Shuffle the few most distinctive ones so siblings don't all get the same name.
    let top = scored.first().map_or(0.0, |s| s.1);
    let mut strong: Vec<&'static str> = scored
        .iter()
        .take(4)
        .filter(|s| s.1 > 0.25 && s.1 > top - 0.3)
        .map(|s| s.0)
        .collect();
    shuffle(&mut rng, &mut strong);
    out.extend(strong);
    out.extend(scored.iter().filter(|s| s.1 > 0.1).map(|s| s.0));
    let (_, mut colors) = color_words(t);
    shuffle(&mut rng, &mut colors);
    out.extend(colors);
    let mut plain = PLAIN.to_vec();
    shuffle(&mut rng, &mut plain);
    out.extend(plain);
    let mut seen = std::collections::HashSet::new();
    out.retain(|w| seen.insert(*w));
    out
}

/// One bit per kind of part the genome has.
fn part_kinds(genome: &Genome) -> u16 {
    (0..parts::SLOTS)
        .filter_map(|k| PartKind::from_code(genome.0[parts::slot_offset(k)]))
        .fold(0, |bits, kind| bits | 1 << kind as u16)
}

/// Whether `genome`'s main diet differs from `founder`'s, leading the founder's main diet by at
/// least `species.diet_margin` (so near-ties don't flicker between species).
fn diet_switched(founder: &Genome, genome: &Genome, config: &Config) -> bool {
    let was = traits::main_diet(traits::diet(founder, config));
    let caps = traits::diet(genome, config);
    let now = traits::main_diet(caps);
    now != was && caps[now] - caps[was] >= config.species.diet_margin
}

const ROMAN: [&str; 10] = [
    "", " II", " III", " IV", " V", " VI", " VII", " VIII", " IX", " X",
];

impl SpeciesRegistry {
    /// Create the root species for the ancestor genome.
    pub fn new(ancestor: &Genome, config: &Config) -> Self {
        let mut reg = Self::default();
        let genus = reg.new_genus(ancestor, None, config);
        let id = reg.insert_species(ancestor, genus, None, 0, config);
        let s = reg.species.get_mut(&id).unwrap();
        s.established = true;
        s.established_at = Some(0);
        reg
    }

    /// Add another founding lineage (its own genus and established species) and return its id.
    pub fn add_root(&mut self, founder: &Genome, config: &Config, tick: u64) -> u32 {
        let genus = self.new_genus(founder, None, config);
        let id = self.insert_species(founder, genus, None, tick, config);
        let s = self.species.get_mut(&id).unwrap();
        s.established = true;
        s.established_at = Some(tick);
        id
    }

    fn new_genus(&mut self, founder: &Genome, parent: Option<u32>, config: &Config) -> u32 {
        let id = self.next_genus;
        self.next_genus += 1;
        let t = Traits::decode(founder, config);
        // Keep the parent genus's diet root while the diet is the same, so the family shows.
        let inherited = parent.and_then(|p| self.genera.get(&p)).and_then(|g| {
            let same_diet = Traits::decode(&g.founder, config).main_diet() == t.main_diet();
            (same_diet && !g.root.is_empty()).then(|| g.root.clone())
        });
        let mut attempt: u64 = 0;
        let (name, root) = loop {
            // Late attempts give up the inherited root to find a free name.
            let inherit = if attempt < 8 { inherited.as_deref() } else { None };
            let (name, root) =
                genus_name(&t, founder, inherit, mix64(id as u64 ^ 0x6E6E ^ attempt << 20));
            attempt += 1;
            if attempt >= 16 || !self.genera.values().any(|g| g.name == name) {
                break (name, root);
            }
        };
        self.genera.insert(
            id,
            Genus {
                id,
                name,
                founder: *founder,
                root,
            },
        );
        id
    }

    fn insert_species(
        &mut self,
        founder: &Genome,
        genus: u32,
        parent: Option<u32>,
        tick: u64,
        config: &Config,
    ) -> u32 {
        let id = self.next_species;
        self.next_species += 1;
        let t = Traits::decode(founder, config);
        self.species.insert(
            id,
            Species {
                id,
                name: String::new(),
                genus,
                parent,
                founder: *founder,
                founded: tick,
                extinct: None,
                population: 0,
                peak: 0,
                established: false,
                established_at: None,
                color: t.color,
                total_born: 0,
            },
        );
        self.assign_name(id, config);
        id
    }

    fn assign_name(&mut self, id: u32, config: &Config) {
        let s = &self.species[&id];
        let genus_id = s.genus;
        let genus = self.genera[&genus_id].name.clone();
        let t = Traits::decode(&s.founder, config);
        // Only named (established) species matter to viewers; transient variants don't
        // reserve names.
        let taken = |name: &str| {
            self.species
                .values()
                .any(|o| o.id != id && o.established && o.name == name)
        };
        // Compare with the named species it split from (what the family tree shows).
        let parent = s
            .parent
            .and_then(|p| self.established_ancestor(p))
            .and_then(|p| self.species.get(&p))
            .map(|p| (Traits::decode(&p.founder, config), p.founder));
        let candidates = epithet_candidates(
            &t,
            &s.founder,
            parent.as_ref().map(|(pt, pg)| (pt, pg)),
            &genus,
            mix64(id as u64 ^ 0x5EED),
        );
        let mut name = None;
        for e in &candidates {
            let n = format!("{genus} {e}");
            if !taken(&n) {
                name = Some(n);
                break;
            }
        }
        let name = name.unwrap_or_else(|| {
            let base = format!("{genus} {}", candidates[0]);
            (1..)
                .map(|n| {
                    if n < ROMAN.len() {
                        format!("{base}{}", ROMAN[n])
                    } else {
                        format!("{base} {}", n + 1)
                    }
                })
                .find(|n| !taken(n))
                .unwrap()
        });
        self.species.get_mut(&id).unwrap().name = name;
    }

    /// Species for a newborn of `parent_species` with `genome`: the parent's, or a new one.
    pub fn classify(
        &mut self,
        parent_species: u32,
        genome: &Genome,
        tick: u64,
        config: &Config,
    ) -> u32 {
        let sc = &config.species;
        let Some(parent) = self.species.get(&parent_species) else {
            return parent_species;
        };
        if genome.distance(&parent.founder, sc) <= sc.threshold
            && !Self::new_kind(&parent.founder, genome, config)
        {
            return parent_species;
        }
        let parent_genus = parent.genus;
        let genus = match self.genera.get(&parent_genus) {
            Some(g)
                if genome.distance(&g.founder, sc) <= sc.genus_threshold
                    && !diet_switched(&g.founder, genome, config) =>
            {
                parent_genus
            }
            _ => self.new_genus(genome, Some(parent_genus), config),
        };
        self.insert_species(genome, genus, Some(parent_species), tick, config)
    }

    /// Whether `genome` is a different kind of organism from `founder` however close their
    /// genes: it clearly eats a different way, or has a kind of part the founder didn't.
    pub fn new_kind(founder: &Genome, genome: &Genome, config: &Config) -> bool {
        part_kinds(genome) & !part_kinds(founder) != 0 || diet_switched(founder, genome, config)
    }

    pub fn on_birth(&mut self, species: u32, config: &Config, tick: u64) {
        let threshold = config.species.established;
        if let Some(s) = self.species.get_mut(&species) {
            s.population += 1;
            s.total_born += 1;
            s.peak = s.peak.max(s.population);
            if s.extinct.is_some() {
                s.extinct = None;
            }
            if !s.established && s.population >= threshold {
                s.established = true;
                s.established_at = Some(tick);
                // Pick the name now, so it's unique among the named species.
                self.assign_name(species, config);
                self.events.push(SpeciesEvent::Established(species));
            }
        }
    }

    pub fn on_death(&mut self, species: u32, tick: u64) {
        let Some(s) = self.species.get_mut(&species) else {
            return;
        };
        s.population = s.population.saturating_sub(1);
        if s.population == 0 {
            if s.established {
                s.extinct = Some(tick);
                self.events.push(SpeciesEvent::Extinct(species));
            } else {
                // Never got going; forget it. Genera without species are kept (cheap).
                self.species.remove(&species);
            }
        }
    }

    pub fn get(&self, id: u32) -> Option<&Species> {
        self.species.get(&id)
    }

    pub fn name(&self, id: u32) -> &str {
        self.species
            .get(&id)
            .map(|s| s.name.as_str())
            .unwrap_or("?")
    }

    /// Established species currently alive.
    pub fn living(&self) -> impl Iterator<Item = &Species> {
        self.species
            .values()
            .filter(|s| s.established && s.population > 0)
    }

    /// Nearest established ancestor (for unestablished variants).
    pub fn established_ancestor(&self, mut id: u32) -> Option<u32> {
        for _ in 0..10_000 {
            let s = self.species.get(&id)?;
            if s.established {
                return Some(id);
            }
            id = s.parent?;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Config, Genome, SpeciesRegistry) {
        let config = Config::default();
        let g = Genome::parse(&config.population.ancestor).unwrap();
        let reg = SpeciesRegistry::new(&g, &config);
        (config, g, reg)
    }

    #[test]
    fn root_species_is_named() {
        let (_, _, reg) = setup();
        let root = reg.get(0).unwrap();
        assert!(root.established);
        assert!(root.name.starts_with("Chloro"), "{}", root.name);
        assert!(root.name.contains(' '));
    }

    #[test]
    fn small_changes_stay_in_species_large_changes_found_new_ones() {
        let (config, g, mut reg) = setup();
        let mut close = g;
        close.0[gene::SIZE] += 2;
        assert_eq!(reg.classify(0, &close, 10, &config), 0);
        let mut far = g;
        far.0[gene::SIZE] = 0x90;
        far.0[gene::HUNT] = 0xC0;
        far.0[gene::TOXIN] = 0x80;
        far.0[gene::FIXATION] = 0x80;
        far.0[parts::slot_offset(0)] = PartKind::Fin.code();
        // A single huge jump alone is not enough.
        let mut one = g;
        one.0[gene::SIZE] = 0xFF;
        assert_eq!(reg.classify(0, &one, 10, &config), 0);
        let id = reg.classify(0, &far, 10, &config);
        assert_ne!(id, 0);
        let s = reg.get(id).unwrap();
        assert_eq!(s.parent, Some(0));
        assert!(!s.established);
        assert_ne!(s.name, reg.get(0).unwrap().name);
    }

    #[test]
    fn a_new_part_kind_founds_a_species_but_losing_one_doesnt() {
        let (config, g, mut reg) = setup();
        let mut finned = g;
        finned.0[parts::slot_offset(0)] = PartKind::Fin.code();
        assert!(g.distance(&finned, &config.species) <= config.species.threshold);
        let id = reg.classify(0, &finned, 10, &config);
        assert_ne!(id, 0);
        // Its descendants that lose the fin are still the finned species.
        assert_eq!(reg.classify(id, &g, 11, &config), id);
    }

    #[test]
    fn a_clear_diet_switch_founds_a_species_and_genus() {
        let (config, g, mut reg) = setup();
        let mut hunter = g;
        hunter.0[gene::PHOTO] = 0x60;
        hunter.0[gene::HUNT] = 0xC0;
        assert!(g.distance(&hunter, &config.species) <= config.species.threshold);
        assert_eq!(traits::main_diet(traits::diet(&hunter, &config)), 3);
        let id = reg.classify(0, &hunter, 10, &config);
        assert_ne!(id, 0);
        assert_ne!(reg.get(id).unwrap().genus, reg.get(0).unwrap().genus);
        // Barely ahead isn't a switch.
        let mut tied = g;
        tied.0[gene::PHOTO] = 0x80;
        tied.0[gene::HUNT] = 0x84;
        let caps = traits::diet(&tied, &config);
        assert!(caps[3] > caps[0] && caps[3] - caps[0] < config.species.diet_margin);
        assert_eq!(reg.classify(0, &tied, 10, &config), 0);
    }

    #[test]
    fn species_lifecycle() {
        let (config, g, mut reg) = setup();
        let mut far = g;
        far.0[gene::HUNT] = 0xFF;
        far.0[gene::PHOTO] = 0x00;
        far.0[gene::RESPIRATION] = 0x00;
        far.0[gene::SIZE] = 0x80;
        far.0[parts::slot_offset(0)] = PartKind::Jaw.code();
        far.0[parts::slot_offset(1)] = PartKind::Fin.code();
        let id = reg.classify(0, &far, 5, &config);
        assert_ne!(id, 0);
        for _ in 0..config.species.established {
            reg.on_birth(id, &config, 6);
        }
        assert!(reg.get(id).unwrap().established);
        for _ in 0..config.species.established {
            reg.on_death(id, 9);
        }
        assert_eq!(reg.get(id).unwrap().extinct, Some(9));
        // An unestablished variant is forgotten when it dies out.
        let mut other = g;
        other.0[gene::CHEMO] = 0xFF;
        other.0[gene::PHOTO] = 0x00;
        other.0[gene::BUOYANCY] = 0x00;
        other.0[gene::OXYGENIC] = 0x00;
        other.0[gene::TEMP_PREF] = 0xFF;
        other.0[parts::slot_offset(0)] = PartKind::Holdfast.code();
        let id2 = reg.classify(0, &other, 5, &config);
        assert_ne!(id2, 0);
        reg.on_birth(id2, &config, 6);
        reg.on_death(id2, 7);
        assert!(reg.get(id2).is_none());
    }
}
