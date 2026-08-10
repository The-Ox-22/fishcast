use super::SpeciesProfile;

pub const PROFILE: SpeciesProfile = SpeciesProfile {
    id: "largemouth_bass",
    display_name: "Largemouth Bass",
    spawn_temp_range_f: (60.0, 75.0),
    rules_toml: include_str!("../../rules/largemouth_bass.toml"),
};
