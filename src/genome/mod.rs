//! Genome encoding (v2) and phenotype decode.

mod decode;
mod encoding_v2;
mod error;
mod mutation_v2;
mod phenotype_v2;
mod signatures;
mod util;

pub use decode::{
    ancestor_from_config as ancestor_any_from_config, decode_phenotype, decode_v2,
    genome_from_bytes, phenotype_body_radius, phenotype_color, phenotype_max_speed,
    phenotype_repro_threshold_energy, AnyDecodeError, AnyGenome, AnyPhenotype,
};
pub use encoding_v2::{
    core as v2_core, GenomeV2, CORE_FIXED_LEN as V2_CORE_FIXED_LEN, HEADER_LEN as V2_HEADER_LEN,
    MAGIC as V2_MAGIC,
};
pub use error::GenomeError;
pub use mutation_v2::mutate_offspring_v2;
pub use phenotype_v2::PhenotypeV2;
pub use util::{mitigated_damage, wants_to_move_this_tick};
pub use signatures::{
    atom_matches, byte_fuzzy_match, parse_pursue_avoid_lists, parse_slot_list,
    rgb_fuzzy_match, slot_list_matches, slot_matches, u16_fuzzy_match, ChemicalObservation,
    SignatureAtom, SignatureSlot, AND_TYPE, MAX_AND_ARITY, SIG_ARMOR, SIG_BODY_RADIUS,
    SIG_BRIGHTER_SUNLIGHT, SIG_COLOR, SIG_DEAD_ORGANISM, SIG_DIMMER_SUNLIGHT, SIG_MINERALS,
    SIMPLE_SLOT_LEN,
};
pub use signatures::{expected_genome_len, slot_payload_len, variable_payload_len};
