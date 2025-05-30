use axum::{
    extract::{ConnectInfo, Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tracing::{debug, info};

use crate::{dt_format, AppState, Record};
use std::collections::HashSet;

#[derive(Deserialize, Debug)]
pub struct UpdateRequestParams {
    pub version: Option<String>,
    pub host: Option<String>,
    // Handle both single values and arrays
    pub ether: Option<String>,
    #[serde(alias = "ether[]")]
    pub ether_array: Option<Vec<String>>,
    pub inet: Option<String>, 
    #[serde(alias = "inet[]")]
    pub inet_array: Option<Vec<String>>,
    pub inet6: Option<String>,
    #[serde(alias = "inet6[]")] 
    pub inet6_array: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct DumpEntry {
    #[serde(flatten)]
    pub record: Record,
    pub datetime: String,
    pub t: i64,
}

#[derive(Deserialize, Debug)]
pub struct AddHostParams {
    pub host: String,
}

fn merge_single_and_array(single: Option<String>, array: Option<Vec<String>>) -> Option<HashSet<String>> {
    match (single, array) {
        (Some(s), None) => Some([s].into_iter().collect()),
        (None, Some(arr)) => Some(arr.into_iter().collect()),
        (Some(s), Some(mut arr)) => {
            arr.push(s);
            Some(arr.into_iter().collect())
        },
        (None, None) => None,
    }
}

pub async fn root_handler(
    State(state): State<AppState>,
    Query(params): Query<UpdateRequestParams>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let host_name = match params.host {
        Some(h) => h.to_lowercase(),
        None => {
            debug!("No host specified, ignoring");
            return Html(
                "<html><body><p>no host specified</p>
                <p><a href=\"/listhosts\">listhosts</a></p>
                <p><a href=\"/dump\">dump</a></p></body></html>",
            )
            .into_response();
        }
    };

    let client_ip_str = if let Some(x_forwarded_for) = headers.get("x-forwarded-for") {
        if let Ok(xff_str) = x_forwarded_for.to_str() {
            xff_str.split(',').next().unwrap_or("").trim().to_string()
        } else {
            addr.ip().to_string()
        }
    } else {
        addr.ip().to_string()
    };

    let current_record = Record {
        hostname: host_name.clone(),
        version: params.version,
        remote_addr: client_ip_str,
        ether: merge_single_and_array(params.ether, params.ether_array),
        inet: merge_single_and_array(params.inet, params.inet_array),
        inet6: merge_single_and_array(params.inet6, params.inet6_array),
    };

    debug!("Received record: {:?}", current_record);

    let mut records_guard = state.records.write().await;
    let mut datetimes_guard = state.datetimes.write().await;
    let mut timestamps_guard = state.timestamps.write().await;
    let mut changed_hosts_guard = state.changed_hosts.write().await;

    let previous_record = records_guard.get(&host_name);

    if previous_record != Some(&current_record) {
        debug!("Record change for {}: {:?}", host_name, current_record);
        records_guard.insert(host_name.clone(), current_record);
        changed_hosts_guard.insert(host_name.clone());
    }

    let now = chrono::Utc::now();
    datetimes_guard.insert(host_name.clone(), now);
    timestamps_guard.insert(host_name.clone(), now.timestamp());

    "OK".into_response()
}

pub async fn dump_handler(State(state): State<AppState>) -> impl IntoResponse {
    let records_guard = state.records.read().await;
    let datetimes_guard = state.datetimes.read().await;
    let timestamps_guard = state.timestamps.read().await;

    let mut result = String::new();

    for (host, rec_ref) in records_guard.iter() {
        let rec_clone = rec_ref.clone();

        let dump_entry = DumpEntry {
            record: rec_clone,
            datetime: datetimes_guard
                .get(host)
                .map_or_else(String::new, dt_format),
            t: timestamps_guard.get(host).copied().unwrap_or(0),
        };
        
        if let Ok(json_line) = serde_json::to_string(&dump_entry) {
            result.push_str(&json_line);
            result.push('\n');
        }
    }
    result.push('\n');
    
    result
}

pub async fn dump2_handler(State(state): State<AppState>) -> Json<Vec<DumpEntry>> {
    let records_guard = state.records.read().await;
    let datetimes_guard = state.datetimes.read().await;
    let timestamps_guard = state.timestamps.read().await;

    let mut result_list = Vec::new();

    for (host, rec_ref) in records_guard.iter() {
        let rec_clone = rec_ref.clone();

        result_list.push(DumpEntry {
            record: rec_clone,
            datetime: datetimes_guard
                .get(host)
                .map_or_else(String::new, dt_format),
            t: timestamps_guard.get(host).copied().unwrap_or(0),
        });
    }
    Json(result_list)
}

pub async fn addhost_handler(
    State(state): State<AppState>,
    Query(params): Query<AddHostParams>,
) -> Html<String> {
    let host_to_add = params.host.to_lowercase();
    info!("Forced addition of {}", host_to_add);
    state
        .do_pair_hosts
        .write()
        .await
        .insert(host_to_add.clone());
    Html(format!("will add {}", host_to_add))
}

pub async fn listhosts_handler(State(state): State<AppState>) -> Html<String> {
    let records_guard = state.records.read().await;
    let datetimes_guard = state.datetimes.read().await;
    let unpaired_guard = state.unpaired_hosts.read().await;

    let mut ret = String::from("<html><body><table>");
    ret.push_str(
        "<tr><th>hostname</th><th>datetime</th><th>version</th>
        <th>ether</th><th>inet</th><th>inet6</th>
        <th>remote_addr</th><th>ops</th></tr>",
    );

    let mut sorted_hosts: Vec<String> = records_guard.keys().cloned().collect();
    sorted_hosts.sort();

    for host_name in sorted_hosts {
        if let Some(rec) = records_guard.get(&host_name) {
            ret.push_str("<tr>");
            ret.push_str(&format!("<td>{}</td>", rec.hostname));
            ret.push_str(&format!(
                "<td>{}</td>",
                datetimes_guard
                    .get(&host_name)
                    .map_or_else(String::new, dt_format)
            ));
            ret.push_str(&format!(
                "<td>{}</td>",
                rec.version.as_deref().unwrap_or("")
            ));

            for af_val_opt in [&rec.ether, &rec.inet, &rec.inet6] {
                ret.push_str("<td>");
                if let Some(vals_set) = af_val_opt {
                    let mut vals_vec: Vec<String> = vals_set.iter().cloned().collect();
                    vals_vec.sort();
                    ret.push_str(&vals_vec.join("<br/>"));
                }
                ret.push_str("</td>");
            }
            ret.push_str(&format!("<td>{}</td>", rec.remote_addr));

            if unpaired_guard.contains(&host_name) {
                ret.push_str(&format!(
                    "<td><a href=\"/addhost?host={}\">add</a></td>",
                    host_name
                ));
            } else {
                ret.push_str("<td></td>");
            }
            ret.push_str("</tr>");
        }
    }

    ret.push_str("</table></body></html>");
    Html(ret)
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/dump", get(dump_handler))
        .route("/dump2", get(dump2_handler))
        .route("/addhost", get(addhost_handler))
        .route("/listhosts", get(listhosts_handler))
        .with_state(state)
}