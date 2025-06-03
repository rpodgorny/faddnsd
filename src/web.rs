use axum::extract::{Form, RawQuery};
use axum::{
    extract::{ConnectInfo, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use tracing::{debug, info};

use crate::{dt_format, AppState, Record};
use std::collections::HashMap;
use std::collections::HashSet;

const HEADER: &str = r#"
	<!DOCTYPE html>
	<html>
		<head>
			<meta charset='utf-8'>
			<meta name='viewport' content='width=device-width, initial-scale=1'>
			<title>faddnsd</title>
			<link rel='stylesheet' href='./static/output.css'>
			<link rel='stylesheet' href='./static/vendor/fontawesome/css/all.css'>
			<script src='./static/vendor/htmx.min.js'></script>
		</head>
		<body class='bg-gray-100 min-h-screen'>
"#;
const FOOTER: &str = "</body></html>";

#[derive(Debug)]
pub struct UpdateRequestParams {
    pub version: Option<String>,
    pub host: Option<String>,
    pub ether: Vec<String>,
    pub inet: Vec<String>,
    pub inet6: Vec<String>,
}

fn parse_query_params(query: &str) -> UpdateRequestParams {
    let mut params = HashMap::new();

    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            let decoded_value = urlencoding::decode(value)
                .unwrap_or_else(|_| value.into())
                .into_owned();
            params
                .entry(key.to_string())
                .or_insert_with(Vec::new)
                .push(decoded_value);
        }
    }

    let get_first = |key: &str| params.get(key).and_then(|v| v.first()).cloned();
    let get_all = |key: &str| params.get(key).cloned().unwrap_or_default();

    UpdateRequestParams {
        version: get_first("version"),
        host: get_first("host"),
        ether: get_all("ether"),
        inet: get_all("inet"),
        inet6: get_all("inet6"),
    }
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

fn vec_to_hashset_opt(vec: Vec<String>) -> Option<HashSet<String>> {
    if vec.is_empty() {
        None
    } else {
        Some(vec.into_iter().collect())
    }
}

pub async fn root_handler(
    State(state): State<AppState>,
    RawQuery(query): RawQuery,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let params = match query {
        Some(q) => parse_query_params(&q),
        None => UpdateRequestParams {
            version: None,
            host: None,
            ether: Vec::new(),
            inet: Vec::new(),
            inet6: Vec::new(),
        },
    };
    let host_name = match params.host {
        Some(h) => h.to_lowercase(),
        None => {
            debug!("No host specified, ignoring");
            return Html(format!(
                "{HEADER}
                <p>no host specified</p>
                <p><a href=\"/listhosts\">listhosts</a></p>
                <p><a href=\"/dump\">dump</a></p>
                {FOOTER}"
            ))
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
        ether: vec_to_hashset_opt(params.ether),
        inet: vec_to_hashset_opt(params.inet),
        inet6: vec_to_hashset_opt(params.inet6),
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

    let now = chrono::Local::now();
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
    Form(params): Form<AddHostParams>,
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

    let mut ret = String::from(HEADER);
    ret.push_str(
        "<div class='container mx-auto px-2 py-8'>
        <h1 class='text-3xl font-bold text-gray-800 mb-6'>Host List</h1>
        <div class='bg-white rounded-lg shadow-md overflow-hidden'>
        <table class='min-w-full divide-y divide-gray-200'>
        <thead class='bg-gray-50'>
        <tr>
            <th class='px-3 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider'>Hostname</th>
            <th class='px-3 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider'>DateTime</th>
            <th class='px-3 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider'>Version</th>
            <th class='px-3 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider'>Ether</th>
            <th class='px-3 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider'>IPv4</th>
            <th class='px-3 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider'>IPv6</th>
            <th class='px-3 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider'>Remote Addr</th>
            <th class='px-3 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider'>Actions</th>
        </tr>
        </thead>
        <tbody class='bg-white divide-y divide-gray-200'>",
    );

    let mut sorted_hosts: Vec<String> = records_guard.keys().cloned().collect();
    sorted_hosts.sort();

    for host_name in sorted_hosts {
        if let Some(rec) = records_guard.get(&host_name) {
            ret.push_str("<tr class='hover:bg-gray-50'>");
            ret.push_str(&format!(
                "<td class='px-3 py-4 text-sm font-medium text-gray-900 break-words'>{}</td>",
                rec.hostname
            ));
            ret.push_str(&format!(
                "<td class='px-3 py-4 text-sm text-gray-500 break-words'>{}</td>",
                datetimes_guard
                    .get(&host_name)
                    .map_or_else(String::new, dt_format)
            ));
            ret.push_str(&format!(
                "<td class='px-3 py-4 text-sm text-gray-500 break-words'>{}</td>",
                rec.version.as_deref().unwrap_or("")
            ));

            for af_val_opt in [&rec.ether, &rec.inet, &rec.inet6] {
                ret.push_str("<td class='px-3 py-4 text-sm text-gray-500'>");
                if let Some(vals_set) = af_val_opt {
                    let mut vals_vec: Vec<String> = vals_set.iter().cloned().collect();
                    vals_vec.sort();
                    ret.push_str(&format!("<div class='space-y-1'>{}</div>", vals_vec.iter().map(|v| format!("<div class='bg-blue-100 text-blue-800 px-2 py-1 rounded text-sm break-all'>{}</div>", v)).collect::<Vec<_>>().join("")));
                }
                ret.push_str("</td>");
            }
            ret.push_str(&format!(
                "<td class='px-3 py-4 text-sm text-gray-500 break-words'>{}</td>",
                rec.remote_addr
            ));

            if unpaired_guard.contains(&host_name) {
                ret.push_str(&format!(
                    "<td class='px-3 py-4 text-sm font-medium'>
                      <button hx-post='/addhost' hx-vals='{{\"host\":\"{host_name}\"}}' hx-target='this' hx-swap='innerHTML' class='bg-green-600 hover:bg-green-700 text-white px-3 py-1 rounded text-xs'>Add</button>
                    </td>"
                ));
            } else {
                ret.push_str("<td class='px-3 py-4 text-sm font-medium'></td>");
            }
            ret.push_str("</tr>");
        }
    }

    ret.push_str("</tbody></table></div></div>");
    ret.push_str(FOOTER);
    Html(ret)
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/dump", get(dump_handler))
        .route("/dump2", get(dump2_handler))
        .route("/addhost", post(addhost_handler))
        .route("/listhosts", get(listhosts_handler))
        .with_state(state)
        .nest_service("/static", ServeDir::new("static"))
}
