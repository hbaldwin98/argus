use super::*;

#[test]
fn only_https_on_the_three_hosts_is_allowed() {
    // EDITOR-CSP-4: the only hosts the editor talks to are the two jsDelivr
    // hosts and the npm registry.
    assert!(allowed("https://cdn.jsdelivr.net/npm/tree-sitter-rust@0.24.0/tree-sitter-rust.wasm"));
    assert!(allowed("https://data.jsdelivr.com/v1/packages/npm/x@1?structure=flat"));
    assert!(allowed("https://registry.npmjs.org/-/v1/search?text=tree-sitter-kotlin&size=10"));
}

#[test]
fn another_host_scheme_or_lookalike_is_refused() {
    assert!(!allowed("http://cdn.jsdelivr.net/npm/x"), "plain http");
    assert!(!allowed("https://example.com/x.wasm"));
    assert!(!allowed("https://cdn.jsdelivr.net.evil.example/x"), "a suffix is not the host");
    assert!(!allowed("https://evil.example/https://cdn.jsdelivr.net/x"), "a host in the path is not the host");
    assert!(!allowed("file:///etc/passwd"));
    assert!(!allowed("not a url"));
    assert!(!allowed(""));
}

#[test]
fn a_foreign_address_is_refused_without_a_request() {
    let said = Net::new().get("https://example.com/x", 10).unwrap_err();
    assert!(said.contains("is not an address Argus fetches grammars from"), "{said}");
}
