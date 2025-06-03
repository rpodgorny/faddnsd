use chrono::{DateTime, Local};
use faddnsd::{generate_bind_lines_for_record, is_ip_restricted, Record};
use std::collections::HashSet;
use std::fs;

#[test]
fn test_is_ip_restricted() {
    // Public IPv4
    assert!(!is_ip_restricted("8.8.8.8"));
    assert!(!is_ip_restricted("46.36.37.83"));
    
    // Private IPv4
    assert!(is_ip_restricted("192.168.1.1"));
    assert!(is_ip_restricted("10.0.0.1"));
    assert!(is_ip_restricted("172.16.0.1"));
    
    // Loopback
    assert!(is_ip_restricted("127.0.0.1"));
    assert!(is_ip_restricted("::1"));
    
    // Public IPv6
    assert!(!is_ip_restricted("2002:2e24:2741:9900::1"));
    assert!(!is_ip_restricted("2a02:25b0:aaaa:5555::1111"));
    
    // Link-local IPv6
    assert!(is_ip_restricted("fe80::1"));
    
    // ULA IPv6
    assert!(is_ip_restricted("fc00::1"));
    assert!(is_ip_restricted("fd00::1"));
    
    // Invalid IP
    assert!(is_ip_restricted("not.an.ip"));
    assert!(is_ip_restricted(""));
}

#[test]
fn test_generate_bind_lines_for_record() {
    let dt = DateTime::parse_from_rfc3339("2025-05-30T10:00:00Z")
        .unwrap()
        .with_timezone(&Local);
    
    let mut inet_set = HashSet::new();
    inet_set.insert("46.36.37.83".to_string());
    inet_set.insert("192.168.1.1".to_string()); // This should be filtered out
    
    let mut inet6_set = HashSet::new();
    inet6_set.insert("2a02:25b0:aaaa:5555::1111".to_string());
    inet6_set.insert("fe80::1".to_string()); // This should be filtered out
    
    let record = Record {
        hostname: "testhost".to_string(),
        version: Some("1.0".to_string()),
        remote_addr: "1.2.3.4".to_string(),
        ether: None,
        inet: Some(inet_set),
        inet6: Some(inet6_set),
    };
    
    let result = generate_bind_lines_for_record(&record, &dt);
    
    // Should contain the public IPv4 (timestamp will vary based on local timezone)
    assert!(result.contains("testhost\t10M\tA\t46.36.37.83 ; @faddns"));
    // Should contain the public IPv6 (timestamp will vary based on local timezone) 
    assert!(result.contains("testhost\t10M\tAAAA\t2a02:25b0:aaaa:5555::1111 ; @faddns"));
    // Should NOT contain private IPs
    assert!(!result.contains("192.168.1.1"));
    assert!(!result.contains("fe80::1"));
}

#[test]
fn test_zone_file_parsing() {
    let zone_content = fs::read_to_string("test_data/cz.podgorny.zone")
        .expect("Failed to read test zone file");
    
    // Verify basic zone file structure
    assert!(zone_content.contains("SOA"));
    assert!(zone_content.contains("NS"));
    assert!(zone_content.contains("plukovnik"));
    assert!(zone_content.contains("admiral"));
    
    // Verify IPv6 addresses are present
    assert!(zone_content.contains("2a02:25b0:aaaa:5555::1111"));
    assert!(zone_content.contains("2a02:25b0:aaaa:5555::2222"));
    
    // Verify includes are present
    assert!(zone_content.contains("$include cz.podgorny_faddns.zone"));
}

#[test]
fn test_faddns_zone_file_parsing() {
    let faddns_content = fs::read_to_string("test_data/cz.podgorny_faddns.zone")
        .expect("Failed to read test faddns zone file");
    
    // Verify @faddns entries are present
    assert!(faddns_content.contains("@faddns"));
    
    // Count lines with @faddns
    let faddns_lines: Vec<&str> = faddns_content
        .lines()
        .filter(|line| line.contains("@faddns"))
        .collect();
    
    // Should have many dynamic DNS entries
    assert!(faddns_lines.len() > 40, "Expected more than 40 @faddns entries, found {}", faddns_lines.len());
    
    // Verify specific entries exist
    assert!(faddns_content.contains("simir"));
    assert!(faddns_content.contains("milan"));
    assert!(faddns_content.contains("plukovnik"));
    
    // Verify TTL format
    assert!(faddns_content.contains("10M"));
    
    // Verify IPv6 addresses
    assert!(faddns_content.contains("2002:2e24:2741:9900:baae:edff:fe70:a6fd"));
    assert!(faddns_content.contains("2a01:9422:904:1ee:ba27:ebff:fee6:bdc2"));
}

#[test]
fn test_zone_file_has_recent_entries() {
    let faddns_content = fs::read_to_string("test_data/cz.podgorny_faddns.zone")
        .expect("Failed to read test faddns zone file");
    
    // Verify we have recent entries (2025)
    assert!(faddns_content.contains("2025-05-"), "Zone file should contain 2025 entries");
    
    // Parse timestamps to verify they're reasonable
    let lines_with_timestamps: Vec<&str> = faddns_content
        .lines()
        .filter(|line| line.contains("@faddns 2025-"))
        .collect();
    
    assert!(lines_with_timestamps.len() > 10, "Expected recent 2025 entries");
}

#[test]
fn test_zone_file_structure_validation() {
    let zone_content = fs::read_to_string("test_data/cz.podgorny.zone")
        .expect("Failed to read test zone file");
    
    // Verify DNS record types are present
    let record_types = ["A", "AAAA", "CNAME", "MX", "NS", "SOA", "TXT"];
    for record_type in record_types {
        assert!(zone_content.contains(record_type), "Zone file should contain {} records", record_type);
    }
    
    // Verify we have proper TTL values
    assert!(zone_content.contains("$TTL"));
    
    // Verify we have proper comments
    assert!(zone_content.contains(";"));
    
    // Verify the zone includes the faddns zone
    assert!(zone_content.contains("cz.podgorny_faddns.zone"));
}

#[test]
fn test_extract_hostnames_from_faddns_zone() {
    let faddns_content = fs::read_to_string("test_data/cz.podgorny_faddns.zone")
        .expect("Failed to read test faddns zone file");
    
    let mut hostnames = HashSet::new();
    
    for line in faddns_content.lines() {
        if line.contains("@faddns") && !line.trim().is_empty() {
            if let Some(hostname) = line.split_whitespace().next() {
                hostnames.insert(hostname.to_string());
            }
        }
    }
    
    // Verify we extracted a reasonable number of hostnames
    assert!(hostnames.len() > 30, "Expected more than 30 unique hostnames, found {}", hostnames.len());
    
    // Verify specific hostnames are present
    let expected_hosts = ["simir", "milan", "berta", "pokuston", "chuck", "milhouse"];
    for host in expected_hosts {
        assert!(hostnames.contains(host), "Expected to find hostname: {}", host);
    }
}

#[test]
fn test_ipv6_addresses_in_faddns_zone() {
    let faddns_content = fs::read_to_string("test_data/cz.podgorny_faddns.zone")
        .expect("Failed to read test faddns zone file");
    
    let ipv6_count = faddns_content
        .lines()
        .filter(|line| line.contains("AAAA") && line.contains("@faddns"))
        .count();
    
    // Most entries should be IPv6
    assert!(ipv6_count > 40, "Expected more than 40 IPv6 entries, found {}", ipv6_count);
    
    // Verify we have different IPv6 prefixes (indicating diverse networks)
    let prefixes = ["2002:", "2a01:", "2a02:", "2001:"];
    for prefix in prefixes {
        assert!(faddns_content.contains(prefix), "Should contain IPv6 prefix: {}", prefix);
    }
}

#[test]
fn test_no_private_ips_in_faddns_zone() {
    let faddns_content = fs::read_to_string("test_data/cz.podgorny_faddns.zone")
        .expect("Failed to read test faddns zone file");
    
    // Check that there are no private IPv4 addresses in the zone
    let private_patterns = ["192.168.", "10.", "172.16.", "172.17.", "172.18.", "172.19.", 
                           "172.20.", "172.21.", "172.22.", "172.23.", "172.24.", "172.25.",
                           "172.26.", "172.27.", "172.28.", "172.29.", "172.30.", "172.31."];
    
    for pattern in private_patterns {
        assert!(!faddns_content.contains(pattern), 
                "Zone file should not contain private IP pattern: {}", pattern);
    }
    
    // Check that there are no link-local IPv6 addresses
    assert!(!faddns_content.contains("fe80:"), 
            "Zone file should not contain link-local IPv6 addresses");
}

#[test]
fn test_timestamp_format_in_faddns_zone() {
    let faddns_content = fs::read_to_string("test_data/cz.podgorny_faddns.zone")
        .expect("Failed to read test faddns zone file");
    
    // Find lines with timestamps
    let timestamp_lines: Vec<&str> = faddns_content
        .lines()
        .filter(|line| line.contains("@faddns 2"))
        .collect();
    
    assert!(timestamp_lines.len() > 0, "Should have lines with timestamps");
    
    // Verify timestamp format (YYYY-MM-DD HH:MM:SS)
    for line in timestamp_lines.iter().take(5) { // Check first 5
        assert!(line.contains("@faddns 20"), "Should contain year");
        // Look for the timestamp pattern
        let parts: Vec<&str> = line.split("@faddns ").collect();
        if parts.len() > 1 {
            let timestamp_part = parts[1];
            assert!(timestamp_part.len() >= 19, "Timestamp should be at least 19 chars: {}", timestamp_part);
            assert!(timestamp_part.contains("-"), "Timestamp should contain dashes");
            assert!(timestamp_part.contains(":"), "Timestamp should contain colons");
        }
    }
}