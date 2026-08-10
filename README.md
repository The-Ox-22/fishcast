# fishcast
Fish bait suggester

A Rust/axum API that suggests largemouth bass baits, retrieve styles, and
structure to target, based on location plus live-fetched or caller-supplied
conditions. Domain research and full technical design live in
[`docs/research.md`](docs/research.md) and [`docs/design.md`](docs/design.md).

## Known issues / concerns

**Data source gaps**
- USACE CWMS (the reservoir-elevation source design.md originally called
  for) turned out to have no lat/lon or bbox search at all — locations are
  only queryable by USACE district office, with no API to resolve which
  office covers a given point. Dropped for v1 (see `src/water.rs`'s module
  doc comment); reservoir coverage now comes only opportunistically through
  USGS, which does monitor some reservoirs directly but far from all of
  them.
- `water_level_trend` is derived from gage height where available, discharge
  as a fallback proxy otherwise — neither is literally "reservoir pool
  elevation," so treat it as directionally useful rather than precise.
- USGS gauge coverage is real but sparse away from named rivers/major
  reservoirs — most ponds and many small lakes will have no nearby gauge at
  all, and `water_temp_f`/`flow_cfs`/`water_level_trend` will come back
  absent (by design — see the no-gauge-nearby test in `src/water.rs`).

**Tuning caveats** — all of the following are reasonable, literature-grounded
starting points, not values validated against real fishing outcomes yet:
- Pressure-trend threshold (2mb/6h), temp-trend thresholds (4°F mild /
  8°F strong), and water-level-trend threshold (5% relative change).
- Rule weights in `rules/largemouth_bass.toml`, and the weight → confidence
  bucket cutoffs (`high`>=8, `medium`>=4) in `src/rules.rs`.
- The season_phase algorithm (temp trend as primary signal, calendar as
  fallback) is a defensible heuristic, not exhaustively validated against
  real seasonal transitions across regions.
- Solunar major/minor periods approximate moon transit as the midpoint
  between moonrise/moonset, since the free sun/moon API doesn't expose true
  transit time — a standard simplification, not true ephemeris computation.

**Behavioral notes**
- All three live fetches (weather/water/solunar) always run, even when a
  request overrides every field a given fetch would supply — each fetch
  also supplies non-overridable fields (e.g. water.rs's `flow_cfs`), so
  skipping the call would silently drop those. Overrides win in the merge
  step instead; this is a minor efficiency tradeoff, not a correctness one.
- Only largemouth bass ships today (schema supports more species — see
  `src/species/`). The `at` field is accepted and validated but always
  resolves conditions as of actual request time; forward-looking requests
  aren't implemented. No persistence, no auth beyond whatever the cluster/
  ingress provides.

## Future work / improvements

- **Outcome-based tuning.** No logging of past suggestions or their real
  results exists yet, and partial data (suggestion without a known outcome)
  isn't useful on its own. Once there's a real way to capture whether a
  suggestion actually worked, a **contextual bandit** or simple supervised
  scoring model (predict catch-probability per bait category from the same
  tag features) is the right next step for this decision shape — better
  suited than full Q-learning/RL, which assumes a sequential
  state-transition problem this isn't.
- **Optional AI-narrative response mode** — an opt-in `format=narrative`
  param where an LLM call rephrases the same rule-engine decision into
  prose, never deciding anything itself. Deferred, not rejected — see
  design.md §6.
- **Claude Skill companion** — a chat-based interface reusing
  `rules/*.toml` as its knowledge base, alongside the API rather than
  instead of it.
- **Species expansion** — add a new `SpeciesProfile` + rules TOML per
  species; no engine changes needed.
- **USACE CWMS via a static lookup table** — a shipped lat/lon → USACE
  district office mapping would make a real CWMS integration for reservoir
  elevation feasible without a live geographic search.
- **Runtime-loaded rules** — swap the compile-time `include_str!` for a
  ConfigMap-mounted file path if hot-tuning rules without a rebuild+
  redeploy ever becomes worth the added Helm/startup-failure surface.
- **Location UX** — lake search or a map-click UI both just need to
  produce a `lat`/`lon` into the existing request shape; no API change
  anticipated.
- **Forward-looking requests** — the `at` field already exists in the API
  shape for this; implementing it means swapping the historical-lookback
  fetch for an actual forecast fetch.
