# Design: fishcast v1

Builds on [`research.md`](./research.md). Scope: largemouth bass, one HTTP
API, current-conditions-plus-recent-trend (no forward forecasting yet, no
map pins/bathymetry).

## 1. Decisions from research

- **Input mode: hybrid.** Caller gives a location; the service auto-fetches
  what it can (weather, water temp where a gauge exists, solunar). Fields
  with no free live source (water clarity, water body type, cover) are
  caller-supplied. Any auto-fetched field can be overridden by the caller
  (they're standing at the water, we're not).
- **Time scope: current + recent trend, not forward forecast.** A request
  is "what should I throw right now," informed by the last ~1-2 weeks of
  conditions, not "what should I throw Saturday." Trend is a *lookback*
  (real historical data, cheap and accurate) rather than a *lookahead*
  (a forecast, which is a different and harder problem) — same v1
  complexity budget, meaningfully better signal. Forward-looking is a
  clean later extension (swap the historical pull for a forecast pull,
  same shape).
- **Rules as data, not code.** Condition → suggestion logic lives in a
  TOML rule table per species, loaded at startup. Adding a species or
  retuning a threshold is a data change, not a Rust change.

## 2. Domain model

### Species

A small trait/table, not hardcoded bass logic scattered through the
engine:

```rust
struct SpeciesProfile {
    id: String,               // "largemouth_bass"
    display_name: String,
    spawn_temp_range_f: (f32, f32),   // drives phase derivation
    rules_file: PathBuf,              // rules/largemouth_bass.toml
}
```

v1 ships one entry (`largemouth_bass`). Adding species #2 later is: a new
`SpeciesProfile` + a new rules TOML — no engine changes.

### Conditions (the shared vocabulary between fetchers and rules)

Every condition field is one of three states after resolution:
`Fetched(value)`, `Provided(value)` (caller override), or `Unknown` (no
source, not supplied — rules that need it just don't match). Wire tags:

| Field | Source | Notes |
|---|---|---|
| `air_temp_f`, `sky`, `wind_mph`, `wind_direction`, `pressure_mb`, `precip_recent` | Open-Meteo (current) | auto |
| `pressure_trend` | Open-Meteo (last 48h, derived) | auto — see §4.1 |
| `temp_trend` | Open-Meteo (historical, derived) | auto — see §4.2 |
| `water_temp_f` | USGS Water Data, if a gauge is within 25mi of location (see §4.3) | auto where available, else caller must supply |
| `water_level_trend`, `flow_cfs` | USGS Water Data (rivers: discharge/gage height, same gauge as water temp) + USACE CWMS Data API (reservoir elevation, Corps-managed lakes only) as a second best-effort source | no single source covers all lakes (Bureau of Reclamation lakes, unmonitored ponds, etc. have nothing) — falls back to caller-supplied `water_level_trend` (rising/falling/stable) when neither source has coverage |
| `solunar_period` | computed from sun/moon rise/set (sunrise-sunset.io) | auto |
| `time_of_day` | derived from request time + sunrise/sunset | auto |
| `season_phase` | derived from `water_temp_f` (or `air_temp_f` as fallback) vs species' `spawn_temp_range_f` | derived, not fetched |
| `water_clarity` | clear / stained / muddy | caller-supplied, no live source |
| `water_body_type` | pond / natural_lake / reservoir / river | caller-supplied, no live source |
| `cover` | list: vegetation, laydowns, riprap, docks, timber, none-known | caller-supplied |
| `water_level_trend` | rising / falling / stable | caller-supplied (reservoir-relevant; skip if not applicable) |

`season_phase`, `pressure_trend`, and `temp_trend` are *derived* fields —
computed once during resolution, then treated as regular condition tags by
the rule engine. This keeps the rule table working off simple categorical
tags rather than each rule re-deriving phase/trend from raw values.

Note these are two distinct, real phenomena with different timescales, not
one "trend" concept split awkwardly in two:

- **Pressure trend** is the classic hours-to-a-day "cold front" signal —
  falling pressure triggers aggressive pre-frontal feeding, a sharp rise
  after a low produces the well-known tough post-frontal bite.
- **Temp trend** is the slower, days-to-weeks acclimation question you
  raised — has water/air been running warmer or colder than it has been,
  independent of today's pressure reading. Both feed the rule engine as
  separate tags since a rule can care about one, the other, or both (e.g.
  "post-frontal AND still in a cold snap" is a stronger tough-bite signal
  than either alone).

### Rule engine

Rules match on a set of **tags** derived from resolved conditions, not on
raw values directly — keeps the TOML declarative instead of embedding
comparison logic in data.

Tag derivation (Rust, one function per axis, e.g. `temp_band(68.0) ->
"warm"`): season_phase (`pre_spawn`/`spawn`/`post_spawn`/`summer`/`fall`/
`winter`), light (`low`/`bright`/`overcast`), wind (`calm`/`light`/
`moderate`/`windy`), clarity (`clear`/`stained`/`muddy`), water_body_type
(pass-through), pressure_trend (`falling`/`rising`/`stable`), temp_trend
(`stable`/`warming`/`cooling`/`cold_front_recent`/`recovering_from_front`),
solunar (`major`/`minor`/`neutral`). Thresholds for the two trend axes are
sourced, not guessed — see §4.

Rule shape (TOML):

```toml
[[rule]]
id = "post_frontal_tough_bite"
requires = ["light:bright", "trend:post_cold_snap"]
weight = 10
baits = [
  { category = "finesse_worm", rig = "shaky_head", colors = ["green_pumpkin", "watermelon"] },
  { category = "jig", variant = "football", colors = ["brown_orange", "green_pumpkin"] },
]
retrieve = "slow, deadstick pauses"
target_structure = ["deeper cover fish pulled to after the front", "shaded docks"]
why = "Post-frontal bright skies + a recent cold snap - fish are tight to cover and slow"

[[rule]]
id = "stained_water_wind_reaction"
requires = ["clarity:stained", "wind:moderate"]
weight = 8
baits = [
  { category = "spinnerbait", colors = ["chartreuse_white"] },
  { category = "chatterbait", colors = ["chartreuse_white", "black_blue"] },
]
retrieve = "steady, moderate speed"
target_structure = ["wind-blown banks", "vegetation edges"]
why = "Wind + stained water hide the bait's flaws - reaction baits shine"
```

Matching: a rule fires if every tag in `requires` is present in the
resolved tag set. All firing rules are returned, sorted by `weight` desc;
engine takes the top N (config, default 3-5) and merges/dedupes bait
categories across them. Each output item carries its source rule's `why`
string so the response is explainable, not a black box.

This is a simple forward-chaining match, not a general expert-system
solver — deliberately, since the tag space is small (~7 axes, each with
3-5 values) and full generality isn't needed.

## 3. API

Single endpoint, POST because the input is a nested object with many
optional fields (query-string GET gets awkward past ~3 fields):

```
POST /api/v1/suggest

{
  "species": "largemouth_bass",           // optional, defaults to largemouth_bass
  "location": { "zip": "74074" },          // or { "lat": 36.13, "lon": -97.07 }
  "at": "2026-08-10T14:00:00-05:00",        // optional, defaults to now
  "conditions": {                           // all optional overrides
    "water_clarity": "stained",
    "water_body_type": "reservoir",
    "cover": ["riprap", "docks"],
    "water_temp_f": 84.0
  }
}
```

Response:

```json
{
  "location": { "lat": 36.13, "lon": -97.07, "resolved_name": "Stillwater, OK" },
  "resolved_conditions": {
    "air_temp_f": { "value": 91.0, "source": "fetched" },
    "water_temp_f": { "value": 84.0, "source": "provided" },
    "pressure_trend": { "value": "falling", "source": "derived" },
    "temp_trend": { "value": "cold_front_recent", "source": "derived" },
    "water_clarity": { "value": "stained", "source": "provided" },
    "water_body_type": { "value": "reservoir", "source": "provided" }
  },
  "season_phase": "summer",
  "suggestions": [
    {
      "bait_category": "chatterbait",
      "colors": ["chartreuse_white"],
      "retrieve": "steady, moderate speed",
      "confidence": "high",
      "why": ["Wind + stained water hide the bait's flaws - reaction baits shine"]
    }
  ],
  "target_structure": [
    { "feature": "wind-blown banks", "why": ["stained_water_wind_reaction"] }
  ]
}
```

Plus `GET /healthz` (already in place) and `GET /api/v1/species` (lists
supported species — trivial, but needed since the client is expected to
let a user pick a species even though only one exists today).

**Location roadmap**: `location` accepts `zip` (coarse, just enough to show
something the moment an app opens) or `lat`/`lon` (precise). That's the
whole contract a future lake-search or click-a-map feature needs — either
one just becomes another way to produce a `lat/lon` before calling this
API. No schema change anticipated when those land.

## 4. Trend computation

Both trend axes are computed from Open-Meteo alone — no second provider
needed. NOAA does publish real 1991-2020 climate normals, but only via a
station-based web portal/bulk CSV, not a coordinate-based API, so it's a
poor fit for a per-request lookup; Open-Meteo's historical archive lets us
compute our own baseline for the exact lat/lon on demand instead.

### 4.1 Pressure trend (fast, ~48h window)

Pull Open-Meteo hourly pressure for the trailing 48 hours. Compare the
most recent 6-hour mean against the prior 6-hour mean 24-42h back:

- falling ≥ 2 mb over that span → `falling` (pre-frontal — the classic
  "feeding frenzy ahead of a front" window)
- rising ≥ 2 mb after having been at a local low in the trailing 48h →
  `rising` (post-frontal — the tough, high-pressure bluebird-sky bite)
- otherwise → `stable`

(2 mb / ~6h is a conservative, commonly-cited threshold for a
meteorologically real pressure swing rather than instrument noise; open to
retuning once we see it against real data, but it's a real starting point,
not a guess pulled from nowhere.)

### 4.2 Temp trend (slow, ~1-2 week window)

This is the one you asked about — "it's been hot, but cooling off for a
few days, does that matter." Pull Open-Meteo historical daily mean temp
(water temp from USGS if a gauge is available, else air temp as a proxy)
for the trailing 6 days, split into two 3-day windows (days -6..-3 and
-3..0), compare means. Thresholds are grounded in what bass-fishing
sources consistently report for cold-front water-temp swings — a typical
front drops water temp 3-5°F, a strong front 6-8°F+, with the tough bite
concentrated in the 1-3 days after the drop before fish re-acclimate
([BassResource](https://www.bassresource.com/fishing/summer-cold-fronts.html),
[Bassmaster](https://www.bassmaster.com/go-fish/news/catching-fish-in-cold-front-conditions-takes-strategy/)):

- `|delta| < 4°F` → `stable`
- `4°F ≤ delta < 8°F` → `warming` / `cooling` (mild swing, not enough by
  itself to flag a behavior-change event)
- `delta ≤ -8°F` → strong recent drop. Then look at just the last 1-2
  days: if still near the window's low point → `cold_front_recent`
  (classic tough-bite window); if the last 1-2 days are already climbing
  back up → `recovering_from_front`
- `delta ≥ +8°F` → `warming` (rapid warm-up; tagged the same as a mild
  warming trend rather than invented as its own "pre-frontal" category —
  the pressure-trend axis in §4.1 already owns that signal, and I don't
  have literature support for temperature-rise-alone being a distinct
  behavioral trigger the way a sharp drop is)

Water temp (USGS gauge) takes precedence over air temp when both are
available, since it's the more direct signal for what the fish actually
feel.

### 4.3 USGS gauge radius

25 miles, as a starting default (config value, easy to retune) — beyond
that a gauge is more likely measuring a different body of water entirely,
so `water_temp_f` falls back to `Unknown` (or the caller's manual value)
rather than reporting a number that isn't representative.

## 5. Architecture (mirrors gas-tracker's shape)

```
src/
  main.rs           entry, no CLI - just starts the server
  config.rs         figment (unchanged pattern)
  server.rs         axum, routes to handlers below
  location.rs       zip/lat-lon resolution (zip -> lat/lon via geocode, reused pattern from gas-tracker)
  weather.rs         Open-Meteo client: current + historical
  water.rs          USGS Water Data client (temp/flow/gage height) + USACE CWMS client (reservoir level, second best-effort source) — nearest-gauge lookup, current + historical
  solunar.rs         sun/moon rise-set fetch + Knight's solunar algorithm -> major/minor periods
  species/
    mod.rs           SpeciesProfile trait/table
    largemouth_bass.rs   spawn temp range etc.
  conditions.rs      resolves raw fetches + overrides -> ResolvedConditions, derives season_phase/trend
  tags.rs            ResolvedConditions -> tag set
  rules.rs           loads TOML rule file, matches tags -> ranked suggestions
  report.rs          suggest::fetch() - orchestrates location -> conditions -> tags -> rules -> response
rules/
  largemouth_bass.toml
```

Each external fetch (`weather.rs`, `water.rs`, `solunar.rs`) returns
`Option<T>` / a result the caller can treat as "unavailable" rather than
hard-failing the whole request — a missing USGS gauge near a farm pond
shouldn't 500 the API, it should just leave `water_temp_f` as `Unknown`
(or fall back to whatever the caller provided).

## 6. Explicitly out of scope for v1

- Map pins / specific in-lake coordinates — output is structure *types*
  only, confirmed in conversation. No bathymetry integration.
- Forward-looking forecast requests (`"at"` in the future) — the field
  exists in the API shape above for forward compatibility but v1 only
  needs to handle "now."
- Persistence / history / any database — every response computed live on
  request, same starting posture as gas-tracker. Logging requests+outcomes
  for future tuning (or an eventual ML model) is a natural v2 direction,
  not a v1 concern.
- Species beyond largemouth bass — schema supports it, only one rules file
  ships.
- Auth/rate limiting beyond whatever the cluster/ingress already does.
- **AI-narrative response mode** — considered and deliberately deferred, not
  rejected. The rule engine would stay the sole decision-maker either way;
  an LLM call would only ever rephrase already-decided structured output
  into prose (constrained to add nothing new), offered as an opt-in
  `format=narrative` param so the default JSON path stays fast. Worth
  revisiting once the rule engine itself is working and there's a real UI
  that wants prose — it adds a network round-trip (slower than the
  parallel weather/water fetches combined) and a new secret (Anthropic API
  key) that aren't worth carrying before then.

## Open questions

Resolved during design: trend thresholds are now concrete and sourced
(§4.1/§4.2), the USGS gauge radius default is 25mi (§4.3), and location
precision is settled — zip is a v1 convenience for "show something
immediately," lat/lon is the precise path, and both future lake-search and
map-click features just produce a lat/lon into the same field (§3).

Nothing outstanding blocks moving to an implementation plan. The one thing
worth flagging as we build rather than deciding now: the §4.1/§4.2
thresholds are literature-grounded starting points, not tuned against real
fishing outcomes — expect to revisit them once the service has actually
been used for a season.
