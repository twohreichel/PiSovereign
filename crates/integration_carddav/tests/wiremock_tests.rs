//! Integration tests for the CardDAV client using WireMock

use wiremock::matchers::{body_string_contains, header, method, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use integration_carddav::client::{CardDavClient, CardDavConfig, HttpCardDavClient};

/// Helper to create a test client against a mock server
#[allow(clippy::expect_used)]
fn test_client(server_url: &str) -> HttpCardDavClient {
    let config = CardDavConfig {
        server_url: server_url.to_string(),
        username: "testuser".to_string(),
        password: "testpass".to_string(),
        addressbook_path: None,
        verify_certs: false,
        timeout_secs: 5,
    };
    HttpCardDavClient::new(config).expect("test client")
}

fn sample_vcard_xml(uid: &str, name: &str) -> String {
    format!(
        r"BEGIN:VCARD
VERSION:3.0
UID:{uid}
FN:{name}
N:{name};;;
EMAIL;TYPE=work:{uid}@example.com
TEL;TYPE=cell:+49123456
END:VCARD"
    )
}

fn multistatus_response(vcards: &[String]) -> String {
    let responses: String = vcards
        .iter()
        .map(|vc| {
            format!(
                r"<D:response>
  <D:propstat>
    <D:prop>
      <card:address-data>{vc}</card:address-data>
    </D:prop>
    <D:status>HTTP/1.1 200 OK</D:status>
  </D:propstat>
</D:response>"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
{responses}
</D:multistatus>"#
    )
}

fn addressbook_propfind_response(hrefs: &[&str]) -> String {
    let responses: String = hrefs
        .iter()
        .map(|href| {
            format!(
                r"<D:response>
  <D:href>{href}</D:href>
  <D:propstat>
    <D:prop>
      <D:resourcetype>
        <D:collection/>
        <card:addressbook/>
      </D:resourcetype>
      <D:displayname>Default</D:displayname>
    </D:prop>
    <D:status>HTTP/1.1 200 OK</D:status>
  </D:propstat>
</D:response>"
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
{responses}
</D:multistatus>"#
    )
}

// === list_addressbooks Tests ===

#[tokio::test]
async fn list_addressbooks_success() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("PROPFIND"))
        .and(header("Depth", "1"))
        .respond_with(
            ResponseTemplate::new(207).set_body_string(addressbook_propfind_response(&[
                "/dav.php/addressbooks/testuser/default/",
            ])),
        )
        .mount(&server)
        .await;

    let result = client.list_addressbooks().await;
    assert!(result.is_ok());
    let books = result.expect("addressbooks");
    assert_eq!(books.len(), 1);
    assert_eq!(books[0], "/dav.php/addressbooks/testuser/default/");
}

#[tokio::test]
async fn list_addressbooks_unauthorized() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("PROPFIND"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let result = client.list_addressbooks().await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(
        err,
        integration_carddav::CardDavError::AuthenticationFailed
    ));
}

#[tokio::test]
async fn list_addressbooks_server_error() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("PROPFIND"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let result = client.list_addressbooks().await;
    assert!(result.is_err());
}

// === get_contacts Tests ===

#[tokio::test]
async fn get_contacts_success() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    let vcards = vec![
        sample_vcard_xml("uid-1", "Max Mustermann"),
        sample_vcard_xml("uid-2", "Erika Muster"),
    ];

    Mock::given(method("REPORT"))
        .and(body_string_contains("addressbook-query"))
        .respond_with(ResponseTemplate::new(207).set_body_string(multistatus_response(&vcards)))
        .mount(&server)
        .await;

    let result = client.get_contacts("/addressbooks/testuser/default").await;
    assert!(result.is_ok());
    let contacts = result.expect("contacts");
    assert_eq!(contacts.len(), 2);
    assert_eq!(contacts[0].id, "uid-1");
    assert_eq!(contacts[1].id, "uid-2");
}

#[tokio::test]
async fn get_contacts_empty() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("REPORT"))
        .respond_with(ResponseTemplate::new(207).set_body_string(multistatus_response(&[])))
        .mount(&server)
        .await;

    let result = client.get_contacts("/addressbooks/testuser/default").await;
    assert!(result.is_ok());
    assert!(result.expect("empty").is_empty());
}

#[tokio::test]
async fn get_contacts_not_found() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("REPORT"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let result = client
        .get_contacts("/addressbooks/testuser/nonexistent")
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        integration_carddav::CardDavError::AddressBookNotFound(_)
    ));
}

#[tokio::test]
async fn get_contacts_unauthorized() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("REPORT"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let result = client.get_contacts("/addressbooks/testuser/default").await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        integration_carddav::CardDavError::AuthenticationFailed
    ));
}

// === get_contact Tests ===

#[tokio::test]
async fn get_contact_success() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    let vcard = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:uid-1\r\nFN:Max Mustermann\r\nN:Mustermann;Max;;;\r\nEND:VCARD\r\n";

    Mock::given(method("GET"))
        .and(path_regex(r"uid-1\.vcf$"))
        .respond_with(ResponseTemplate::new(200).set_body_string(vcard))
        .mount(&server)
        .await;

    let result = client
        .get_contact("/addressbooks/testuser/default", "uid-1")
        .await;
    assert!(result.is_ok());
    let contact = result.expect("contact");
    assert_eq!(contact.id, "uid-1");
    assert_eq!(contact.display_name.as_deref(), Some("Max Mustermann"));
}

#[tokio::test]
async fn get_contact_not_found() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let result = client
        .get_contact("/addressbooks/testuser/default", "nonexistent")
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        integration_carddav::CardDavError::ContactNotFound(_)
    ));
}

// === create_contact Tests ===

#[tokio::test]
async fn create_contact_success() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("PUT"))
        .and(header("If-None-Match", "*"))
        .and(path_regex(r"\.vcf$"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;

    let contact = integration_carddav::Contact::new("uid-new", "New Contact")
        .with_email("new@example.com", None);

    let result = client
        .create_contact("/addressbooks/testuser/default", &contact)
        .await;
    assert!(result.is_ok());
    assert_eq!(result.expect("id"), "uid-new");
}

#[tokio::test]
async fn create_contact_204_success() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let contact = integration_carddav::Contact::new("uid-new", "New");

    let result = client
        .create_contact("/addressbooks/testuser/default", &contact)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn create_contact_unauthorized() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let contact = integration_carddav::Contact::new("uid-new", "New");
    let result = client
        .create_contact("/addressbooks/testuser/default", &contact)
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        integration_carddav::CardDavError::AuthenticationFailed
    ));
}

// === update_contact Tests ===

#[tokio::test]
async fn update_contact_success() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("PUT"))
        .and(path_regex(r"uid-1\.vcf$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let contact = integration_carddav::Contact::new("uid-1", "Updated Name");
    let result = client
        .update_contact("/addressbooks/testuser/default", &contact)
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn update_contact_not_found() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let contact = integration_carddav::Contact::new("nonexistent", "X");
    let result = client
        .update_contact("/addressbooks/testuser/default", &contact)
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        integration_carddav::CardDavError::ContactNotFound(_)
    ));
}

// === delete_contact Tests ===

#[tokio::test]
async fn delete_contact_success() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("DELETE"))
        .and(path_regex(r"uid-1\.vcf$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let result = client
        .delete_contact("/addressbooks/testuser/default", "uid-1")
        .await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn delete_contact_not_found() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let result = client
        .delete_contact("/addressbooks/testuser/default", "nonexistent")
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        integration_carddav::CardDavError::ContactNotFound(_)
    ));
}

#[tokio::test]
async fn delete_contact_unauthorized() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    Mock::given(method("DELETE"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let result = client
        .delete_contact("/addressbooks/testuser/default", "uid-1")
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        integration_carddav::CardDavError::AuthenticationFailed
    ));
}

// === search_contacts Tests ===

#[tokio::test]
async fn search_contacts_filters_results() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    let vcards = vec![
        sample_vcard_xml("uid-1", "Max Mustermann"),
        sample_vcard_xml("uid-2", "Erika Muster"),
    ];

    Mock::given(method("REPORT"))
        .respond_with(ResponseTemplate::new(207).set_body_string(multistatus_response(&vcards)))
        .mount(&server)
        .await;

    let result = client
        .search_contacts("/addressbooks/testuser/default", "Max")
        .await;
    assert!(result.is_ok());
    let contacts = result.expect("search results");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].id, "uid-1");
}

#[tokio::test]
async fn search_contacts_no_results() {
    let server = MockServer::start().await;
    let client = test_client(&server.uri());

    let vcards = vec![sample_vcard_xml("uid-1", "Max Mustermann")];

    Mock::given(method("REPORT"))
        .respond_with(ResponseTemplate::new(207).set_body_string(multistatus_response(&vcards)))
        .mount(&server)
        .await;

    let result = client
        .search_contacts("/addressbooks/testuser/default", "NoMatch")
        .await;
    assert!(result.is_ok());
    assert!(result.expect("no results").is_empty());
}
