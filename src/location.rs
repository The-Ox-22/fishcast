use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum LocationInput {
    Zip { zip: String },
    LatLon { lat: f64, lon: f64 },
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvedLocation {
    pub lat: f64,
    pub lon: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ZippopotamResponse {
    places: Vec<ZippopotamPlace>,
}

#[derive(Debug, Deserialize)]
struct ZippopotamPlace {
    #[serde(rename = "place name")]
    place_name: String,
    #[serde(rename = "state abbreviation")]
    state_abbr: String,
    latitude: String,
    longitude: String,
}

pub async fn resolve(client: &reqwest::Client, input: &LocationInput) -> anyhow::Result<ResolvedLocation> {
    match input {
        LocationInput::Zip { zip } => resolve_zip(client, zip).await,
        LocationInput::LatLon { lat, lon } => resolve_lat_lon(*lat, *lon),
    }
}

fn resolve_lat_lon(lat: f64, lon: f64) -> anyhow::Result<ResolvedLocation> {
    if !(-90.0..=90.0).contains(&lat) {
        bail!("latitude {lat} out of range (-90..=90)");
    }
    if !(-180.0..=180.0).contains(&lon) {
        bail!("longitude {lon} out of range (-180..=180)");
    }
    // No reverse-geocode call for direct lat/lon requests - resolved_name
    // is only populated on the zip path, via zippopotam's place name.
    Ok(ResolvedLocation { lat, lon, resolved_name: None })
}

async fn resolve_zip(client: &reqwest::Client, zip: &str) -> anyhow::Result<ResolvedLocation> {
    let url = format!("https://api.zippopotam.us/us/{zip}");
    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("failed to reach zippopotam.us for zip {zip}"))?;

    if !response.status().is_success() {
        bail!("zip code {zip} not found (zippopotam.us returned {})", response.status());
    }

    let parsed: ZippopotamResponse = response
        .json()
        .await
        .context("failed to parse zippopotam.us response")?;

    let place = parsed
        .places
        .into_iter()
        .next()
        .with_context(|| format!("zippopotam.us returned no places for zip {zip}"))?;

    let lat: f64 = place
        .latitude
        .parse()
        .context("failed to parse latitude from zippopotam.us")?;
    let lon: f64 = place
        .longitude
        .parse()
        .context("failed to parse longitude from zippopotam.us")?;

    Ok(ResolvedLocation {
        lat,
        lon,
        resolved_name: Some(format!("{}, {}", place.place_name, place.state_abbr)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lat_lon_passthrough() {
        let loc = resolve_lat_lon(36.13, -97.07).unwrap();
        assert_eq!(loc.lat, 36.13);
        assert_eq!(loc.lon, -97.07);
        assert!(loc.resolved_name.is_none());
    }

    #[test]
    fn lat_out_of_range_rejected() {
        assert!(resolve_lat_lon(91.0, 0.0).is_err());
        assert!(resolve_lat_lon(-91.0, 0.0).is_err());
    }

    #[test]
    fn lon_out_of_range_rejected() {
        assert!(resolve_lat_lon(0.0, 181.0).is_err());
        assert!(resolve_lat_lon(0.0, -181.0).is_err());
    }

    #[test]
    fn boundary_values_accepted() {
        assert!(resolve_lat_lon(90.0, 180.0).is_ok());
        assert!(resolve_lat_lon(-90.0, -180.0).is_ok());
    }

    #[test]
    fn location_input_deserializes_zip_and_lat_lon() {
        let zip: LocationInput = serde_json::from_str(r#"{"zip":"74074"}"#).unwrap();
        assert!(matches!(zip, LocationInput::Zip { zip } if zip == "74074"));

        let latlon: LocationInput = serde_json::from_str(r#"{"lat":36.13,"lon":-97.07}"#).unwrap();
        assert!(matches!(latlon, LocationInput::LatLon { lat, lon } if lat == 36.13 && lon == -97.07));
    }

    #[tokio::test]
    #[ignore = "hits real zippopotam.us - run with `cargo test -- --ignored`"]
    async fn resolve_zip_live() {
        let client = reqwest::Client::new();
        let loc = resolve(
            &client,
            &LocationInput::Zip { zip: "74074".to_string() },
        )
        .await
        .unwrap();
        // Stillwater, OK - sanity bounding box
        assert!((36.0..=36.3).contains(&loc.lat));
        assert!((-97.2..=-96.9).contains(&loc.lon));
        assert!(loc.resolved_name.is_some());
    }
}
