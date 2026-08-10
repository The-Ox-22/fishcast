# Research: bait/pattern suggestion for largemouth bass

Scope for this pass: what's generally known about matching largemouth bass
behavior to conditions, and what live data we could realistically pull to
drive that logic. Not a design doc yet — just the grounding for one.

## 1. Domain knowledge: largemouth bass behavior

Bass behavior is driven mainly by **water temperature** (their metabolism is
temperature-dependent — they're ectotherms) and **light penetration**
(controls how shallow/aggressive they'll feed and how far they can see a
bait). Almost every other variable below is really a proxy for one of those
two things.

### Seasonal pattern (water temp is the real axis, not the calendar date)

| Phase | Approx water temp | Behavior | Typical baits |
|---|---|---|---|
| Winter / cold | <50°F | Lethargic, deep, tight schools, slow metabolism | Slow-rolled jigs, blade baits, drop shot, suspending jerkbaits fished very slow |
| Pre-spawn | 50–60°F | Staging on secondary points/flats near spawning bays, aggressive feeding to build reserves | Lipless crankbaits, jerkbaits, chatterbaits, jigs |
| Spawn | 60–75°F (varies by strain/region) | Shallow, bedding, males guarding fry, females less catchable | Soft plastics (tubes, creature baits) sight-fished to beds, wacky rig |
| Post-spawn | after spawn, warming | Recovery period, scattered, feed on shad spawn if present | Topwater, swim jigs, spinnerbaits |
| Summer | >75°F | Early/late shallow feeding windows, deep/shaded midday, offshore structure | Topwater (dawn/dusk), deep cranks, Carolina rig, football jigs on offshore structure |
| Fall (turnover aftermath) | falling, mixing water column | Chasing shad, can be erratic right at turnover, then aggressive feeding as it stabilizes | Squarebill crankbaits, spinnerbaits, swimbaits — moving baits generally |

Turnover itself (fall, when surface water cools and the thermocline
collapses, mixing the water column) is a well-known short-term negative —
oxygen/temp stratification briefly scrambles, fish go inactive for days.
Worth detecting rather than just inferring from date, if we get water column
data (unlikely at v1 — see gaps below).

### Time of day

Mostly a light-penetration effect, moderated by season/water clarity:
- Dawn/dusk and low light: fish move shallow and feed aggressively —
  topwater, spinnerbaits, buzzbaits.
- Midday/bright sun (especially summer): fish pull to shade, deeper water,
  or thicker cover — jigs, worms, deeper cranks, flipping into cover.
- Overcast days extend the "low light" window most of the day — often the
  best conditions for reaction baits (spinnerbait, chatterbait) all day.

### Weather / barometric pressure

- **Falling pressure ahead of a front**: often triggers aggressive
  pre-frontal feeding — good topwater/reaction bait window.
- **Post-frontal, high/rising pressure, bluebird skies**: classic tough bite
  — fish pull tight to cover/deeper, slow down presentations (finesse
  worms, shaky head, jigs fished slowly).
- **Stable pressure over several days**: fish pattern predictably, whatever
  that pattern is.
- The *trend* (rising/falling/how fast) matters much more than the absolute
  pressure value — this affects what we should fetch/derive, not just
  display.
- **Wind**: pushes baitfish and warmer surface water onto banks, breaks up
  light penetration (bass more comfortable shallow/aggressive), and reduces
  fishes' ability to see lures clearly (bigger/louder/reaction baits become
  more effective — spinnerbaits, chatterbaits, crankbaits). Dead calm +
  bright sun is usually the toughest combo.
- **Rain**: light rain can trigger feeding; heavy rain / runoff muddies
  water and often creates a short-term slow-down followed by a good bite
  once water starts clearing.

### Water clarity

Not in the original list but arguably as important as anything above —
drives both lure *color* and lure *type*:
- Clear water: natural colors (green pumpkin, shad, watermelon), finesse
  presentations, fluorocarbon line, downsizing.
- Stained water: chartreuse, white, brighter/contrast colors; moderate
  vibration baits (spinnerbait, chatterbait).
- Muddy water: dark solid silhouettes (black/blue, black/red), bulkier
  profiles, more vibration/noise (bladed jigs, rattling crankbaits,
  Colorado-blade spinnerbaits) — bass are hunting by feel/vibration more
  than sight.

### Water body type

- **Ponds**: small, shallow, often stained, limited structure diversity —
  fish relate heavily to any available cover (docks, laydowns, single
  brushpile). Bank fishing dominant use case.
- **Natural lakes**: variable depth, natural vegetation lines, points,
  humps. Vegetation edges are often the single best pattern.
- **Reservoirs**: typically the most structure-rich (flooded timber, creek
  channels, standing brush, riprap dams, bridge pilings) — water level
  fluctuation matters a lot here (rising water = fish push into newly
  flooded cover and feed; falling water = fish pull off the bank to the
  first defined break).
- **Rivers**: current is the dominant factor — fish relate to current
  breaks (eddies behind rocks/laydowns, current seams, bridge pilings), not
  to open water the way lake fish do.

### Region / strain

Northern-strain vs Florida-strain (and F1 hybrids) largemouth differ
somewhat in growth and spawn-temperature triggers; regional climate shifts
the *calendar timing* of the seasonal phases above (e.g. Texas pre-spawn
starts weeks before Minnesota) without changing the underlying
temperature-driven logic. Practically: derive "phase" from actual measured
water temp per location rather than hardcoding month-based rules per
region — same model, geography just shifts the input.

### Structure / cover — what to target, not just what to throw

This is a distinct output axis from "which bait": where in the water body to
fish it.

| Feature | When it's productive |
|---|---|
| Shallow flats / spawning bays | Pre-spawn through spawn, warm afternoons in cold months (shallow water warms fastest) |
| Points (main lake / secondary) | Staging areas pre/post-spawn, year-round travel routes |
| Weed lines / vegetation edges | Summer, especially the outside edge in early morning, inside edge as sun gets high |
| Docks / laydowns / brush (shade + ambush structure) | Midday, summer, post-frontal (fish tuck tight to shade/cover) |
| Drop-offs / creek channels / ledges | Summer/winter offshore pattern, especially reservoirs |
| Current breaks (rivers) | Any time water is moving — the whole "where" question in rivers |
| Riprap / rock (dams, bridges) | Bass relate to rock especially in early spring (warms fast) and around baitfish |

### Lure category → condition mapping (summary)

- **Reaction / moving baits** (spinnerbait, chatterbait, squarebill
  crankbait, lipless crankbait): stained water, wind, low light, active
  fish, covering water to locate fish.
- **Topwater** (popper, walking bait, buzzbait, frog): dawn/dusk, calm-ish
  surface, warm water (generally >60°F), over/near cover or baitfish
  activity.
- **Jerkbait**: pre-spawn, clear-to-stained cold water, suspending fish.
  Cadence (pause length) inversely tracks water temp — colder = longer
  pauses.
- **Jig** (flipping/football/swim): year-round "big bass" bait; flipping
  into heavy cover, football jig dragged on offshore rock/gravel in summer,
  swim jig through grass/wood as a reaction bait.
- **Soft plastics** (Texas rig, Carolina rig, drop shot, wacky rig, ned
  rig): tough bite / post-frontal / heavily pressured fish / precise
  bed-fishing; slower, more finesse presentations generally.
- **Deep crankbait**: summer/offshore structure, covering a depth band
  efficiently.

### Retrieve style

Distinct output worth surfacing alongside the bait itself: burn (fast,
reaction, active fish/warm water), steady medium retrieve, slow-roll
(spinnerbait/chatterbait in cold water), twitch-pause / walk-the-dog
(topwater, jerkbait — pause length scales with water temp/activity level),
deadstick (finesse, tough bite), yo-yo / stroke (jigging vertically on
structure).

## 2. Candidate live data sources

| Data | Source | Notes |
|---|---|---|
| Weather forecast/observations (temp, wind, sky cover, precip) | [NWS API](https://www.weather.gov/documentation) (`api.weather.gov`) | Free, no key, US-only, government-run. Point-based (lat/lon → grid → forecast). |
| Weather incl. barometric pressure, historical | [Open-Meteo](https://open-meteo.com/) | Free, no key up to 10k calls/day non-commercial, global, includes pressure and a historical archive back to 1940 — useful for pressure *trend*, not just a snapshot. Also has a marine API. |
| Water temperature, gage height, streamflow | [USGS Water Services](https://waterservices.usgs.gov/) / new [USGS Water Data API](https://api.waterdata.usgs.gov) | Free, no key, real-time + historical, but only at instrumented gauge sites — coverage is real rivers/major reservoirs, not every farm pond. Note: legacy `waterservices.usgs.gov` is being retired (~Q1 2027) in favor of the new Water Data API — worth building against the new one directly. |
| Sunrise/sunset, moon phase/illumination | [sunrise-sunset.org](https://sunrise-sunset.org/api) or [sunrisesunset.io](https://sunrisesunset.io/api/) | Both free, no key, lat/lon based. Gives us the raw inputs to compute a solunar-style major/minor feeding-window score ourselves. |
| Solunar feeding times | No clean free public API found (existing sites like MyFishingForecast, BassForecast, Fishnotify are consumer products, not APIs) | We'd compute this ourselves from sun/moon data — the underlying algorithm (Knight's solunar theory) is well documented and simple (transit/rise/set of sun+moon → major/minor windows). |
| Water body identification (is this a pond/lake/river, name, rough geometry) | USGS National Hydrography Dataset (NHD) — technically retired Oct 2023, superseded by **3D Hydrography Program (3DHP)**, old data still served via ArcGIS MapServer at `hydro.nationalmap.gov` | Usable but clunky (ArcGIS REST, not a clean JSON API) and coverage/attribution quality varies. For v1, simplest path is probably: user names their body of water and picks a type, rather than us resolving it automatically. |
| Bathymetry / underwater structure (drop-offs, brush, channels) | No free public API (Navionics, Fishbrain are closed/consumer-only) | Real gap — see open questions below. |
| Stocking data, lake records | State DNR sites, format varies per state, generally no API | Out of scope for v1 automation; could be static/manual data per lake later. |

## 3. Existing consumer products in this space

Worth knowing about as prior art (not competitors we need to worry about,
but useful for feature ideas): MyFishingForecast, BassForecast (Spot-On
Solunar), FishingReminder, Fishnotify, Fishbrain (the biggest — social +
mapping + forecasts, closed API). All of them center on the same core loop:
location + time → conditions → a "score" and/or suggested times. None that I
found expose species-specific *bait/technique* recommendations as
prominently as you're describing — that's a reasonable differentiator.

## 4. Inputs I'd add to your list

You asked me to flag anything missing:

- **Water clarity/turbidity** — arguably drives lure color/type choice as
  much as anything on your list. No easy live data source (see gaps) but
  important enough to want as a user-supplied input even before we get
  live conditions working.
- **Barometric pressure *trend*** (rising/falling/stable, and how fast),
  not just current pressure — the trend is what actually correlates with
  bite quality.
- **Water level trend** (rising/falling/stable) — big deal on reservoirs
  specifically.
- **Recent rain / runoff** — short-term muddying effect, distinct from sky
  cover today.
- **Current/flow** — only relevant for rivers, but a primary factor when it
  applies.
- **Spawn stage as an explicit derived state** (pre-spawn/spawn/post-spawn),
  computed from water temp + trend rather than left implicit in "time of
  year" — since it's the single biggest behavior switch of the year and
  timing varies a lot by region.

## 5. Open questions for design

- **Manual conditions vs. live scrape, for v1**: given the real data gaps
  (no clean water-clarity source, no free bathymetry/structure API, USGS
  gauge coverage is sparse away from major rivers/reservoirs), a fully
  automatic "just give me a location" flow may only be partially possible
  at first. Worth deciding whether v1 is manual-conditions-in /
  suggestions-out (simplest, no scraping needed at all) with live-lookup
  layered on per-field as sources allow, versus waiting to build until more
  automation is ready.
- **Structure/location suggestions without bathymetry data**: without
  underwater structure data, "target the drop-off at X" isn't possible —
  more realistic near-term output is generic structure-type guidance
  ("target vegetation edges and points" for these conditions) rather than
  specific spots on a specific lake. Worth confirming that's the right bar
  for v1 output.
- **Species expansion**: keeping species as a first-class selectable field
  from day one (even with only largemouth bass rules implemented) seems
  right per your ask — confirms the schema/model should be
  species-parameterized from the start rather than bass-specific
  internally.
