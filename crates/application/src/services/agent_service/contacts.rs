//! Contact management command handlers

use tracing::{info, warn};

use super::{AgentService, ExecutionResult};
use crate::error::ApplicationError;

impl AgentService {
    /// Handle listing contacts (with optional query filter)
    pub(super) async fn handle_list_contacts(
        &self,
        query: Option<&str>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref contact_service) = self.contact_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📇 Contact service not yet configured.".to_string(),
            });
        };

        info!(query = ?query, "Listing contacts");

        match contact_service.list_contacts(query.map(String::from)).await {
            Ok(contacts) => {
                if contacts.is_empty() {
                    let msg = query.map_or_else(
                        || "📇 No contacts found.".to_string(),
                        |q| format!("📇 No contacts found matching '{q}'."),
                    );
                    return Ok(ExecutionResult {
                        success: true,
                        response: msg,
                    });
                }

                let header = query.map_or_else(
                    || format!("📇 **Contacts** ({} total)\n", contacts.len()),
                    |q| {
                        format!(
                            "📇 **Contacts matching '{q}'** ({} found)\n",
                            contacts.len()
                        )
                    },
                );

                let list: Vec<String> = contacts
                    .iter()
                    .map(|c| {
                        let mut parts = vec![format!("• **{}**", c.display_name)];
                        if let Some(ref email) = c.email {
                            parts.push(format!("  📧 {email}"));
                        }
                        if let Some(ref phone) = c.phone {
                            parts.push(format!("  📞 {phone}"));
                        }
                        if let Some(ref org) = c.organization {
                            parts.push(format!("  🏢 {org}"));
                        }
                        parts.join("\n")
                    })
                    .collect();

                Ok(ExecutionResult {
                    success: true,
                    response: format!("{header}\n{}", list.join("\n\n")),
                })
            },
            Err(e) => {
                warn!(error = %e, "Failed to list contacts");
                Ok(ExecutionResult {
                    success: false,
                    response: format!("❌ Failed to list contacts: {e}"),
                })
            },
        }
    }

    /// Handle getting a single contact by ID
    pub(super) async fn handle_get_contact(
        &self,
        contact_id: &str,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref contact_service) = self.contact_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📇 Contact service not yet configured.".to_string(),
            });
        };

        info!(contact_id = %contact_id, "Getting contact details");

        match contact_service.get_contact(contact_id).await {
            Ok(contact) => {
                let mut lines = vec![format!("📇 **{}**\n", contact.display_name)];

                if let Some(ref first) = contact.first_name {
                    if let Some(ref last) = contact.last_name {
                        lines.push(format!("**Name:** {first} {last}"));
                    }
                }

                if !contact.emails.is_empty() {
                    lines.push(format!("**Email:** {}", contact.emails.join(", ")));
                }
                if !contact.phones.is_empty() {
                    lines.push(format!("**Phone:** {}", contact.phones.join(", ")));
                }
                if let Some(ref org) = contact.organization {
                    lines.push(format!("**Organization:** {org}"));
                }
                if let Some(ref title) = contact.title {
                    lines.push(format!("**Title:** {title}"));
                }
                if !contact.addresses.is_empty() {
                    lines.push(format!("**Address:** {}", contact.addresses.join("; ")));
                }
                if let Some(ref birthday) = contact.birthday {
                    lines.push(format!("**Birthday:** {birthday}"));
                }
                if let Some(ref notes) = contact.notes {
                    lines.push(format!("**Notes:** {notes}"));
                }
                if !contact.categories.is_empty() {
                    lines.push(format!("**Categories:** {}", contact.categories.join(", ")));
                }

                Ok(ExecutionResult {
                    success: true,
                    response: lines.join("\n"),
                })
            },
            Err(e) => {
                warn!(error = %e, contact_id = %contact_id, "Failed to get contact");
                Ok(ExecutionResult {
                    success: false,
                    response: format!("❌ Contact not found: {e}"),
                })
            },
        }
    }

    /// Handle searching contacts by query
    pub(super) async fn handle_search_contacts(
        &self,
        query: &str,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref contact_service) = self.contact_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📇 Contact service not yet configured.".to_string(),
            });
        };

        info!(query = %query, "Searching contacts");

        match contact_service.search_contacts(query).await {
            Ok(contacts) => {
                if contacts.is_empty() {
                    return Ok(ExecutionResult {
                        success: true,
                        response: format!("📇 No contacts found matching '{query}'."),
                    });
                }

                let header = format!(
                    "🔍 **Contact search results for '{query}'** ({} found)\n",
                    contacts.len()
                );

                let list: Vec<String> = contacts
                    .iter()
                    .map(|c| {
                        let mut line = format!("• **{}**", c.display_name);
                        if let Some(ref email) = c.email {
                            line.push_str(&format!(" — {email}"));
                        }
                        if let Some(ref org) = c.organization {
                            line.push_str(&format!(" ({org})"));
                        }
                        line
                    })
                    .collect();

                Ok(ExecutionResult {
                    success: true,
                    response: format!("{header}\n{}", list.join("\n")),
                })
            },
            Err(e) => {
                warn!(error = %e, query = %query, "Failed to search contacts");
                Ok(ExecutionResult {
                    success: false,
                    response: format!("❌ Failed to search contacts: {e}"),
                })
            },
        }
    }

    /// Handle creating a contact (called after approval)
    #[allow(dead_code)] // Will be wired in approval flow
    pub(super) async fn handle_create_contact(
        &self,
        name: &str,
        email: Option<&str>,
        phone: Option<&str>,
        organization: Option<&str>,
        birthday: Option<&str>,
        notes: Option<&str>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref contact_service) = self.contact_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📇 Contact service not yet configured.".to_string(),
            });
        };

        let mut new_contact = crate::ports::NewContact::new(name);

        if let Some(e) = email {
            new_contact = new_contact.with_email(e);
        }
        if let Some(p) = phone {
            new_contact = new_contact.with_phone(p);
        }
        if let Some(o) = organization {
            new_contact = new_contact.with_organization(o);
        }
        if let Some(b) = birthday {
            if let Ok(date) = chrono::NaiveDate::parse_from_str(b, "%Y-%m-%d") {
                new_contact = new_contact.with_birthday(date);
            }
        }
        if let Some(n) = notes {
            new_contact = new_contact.with_notes(n);
        }

        info!(name = %name, "Creating contact");

        match contact_service.create_contact(&new_contact).await {
            Ok(id) => Ok(ExecutionResult {
                success: true,
                response: format!("✅ Contact '{name}' created (ID: {id})"),
            }),
            Err(e) => {
                warn!(error = %e, name = %name, "Failed to create contact");
                Ok(ExecutionResult {
                    success: false,
                    response: format!("❌ Failed to create contact: {e}"),
                })
            },
        }
    }

    /// Handle updating a contact (called after approval)
    #[allow(dead_code)] // Will be wired in approval flow
    pub(super) async fn handle_update_contact(
        &self,
        contact_id: &str,
        name: Option<&str>,
        email: Option<&str>,
        phone: Option<&str>,
        organization: Option<&str>,
        notes: Option<&str>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref contact_service) = self.contact_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📇 Contact service not yet configured.".to_string(),
            });
        };

        let mut update = crate::ports::ContactUpdate::new();

        if let Some(n) = name {
            update = update.with_name(n);
        }
        if let Some(e) = email {
            update = update.with_email(e);
        }
        if let Some(p) = phone {
            update = update.with_phone(p);
        }
        if let Some(o) = organization {
            update = update.with_organization(o);
        }
        if let Some(n) = notes {
            update = update.with_notes(n);
        }

        if !update.has_changes() {
            return Ok(ExecutionResult {
                success: false,
                response: "📇 No changes specified for contact update.".to_string(),
            });
        }

        info!(contact_id = %contact_id, "Updating contact");

        match contact_service.update_contact(contact_id, &update).await {
            Ok(()) => Ok(ExecutionResult {
                success: true,
                response: format!("✅ Contact {contact_id} updated successfully."),
            }),
            Err(e) => {
                warn!(error = %e, contact_id = %contact_id, "Failed to update contact");
                Ok(ExecutionResult {
                    success: false,
                    response: format!("❌ Failed to update contact: {e}"),
                })
            },
        }
    }

    /// Handle deleting a contact (called after approval)
    #[allow(dead_code)] // Will be wired in approval flow
    pub(super) async fn handle_delete_contact(
        &self,
        contact_id: &str,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref contact_service) = self.contact_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📇 Contact service not yet configured.".to_string(),
            });
        };

        info!(contact_id = %contact_id, "Deleting contact");

        match contact_service.delete_contact(contact_id).await {
            Ok(()) => Ok(ExecutionResult {
                success: true,
                response: format!("✅ Contact {contact_id} deleted."),
            }),
            Err(e) => {
                warn!(error = %e, contact_id = %contact_id, "Failed to delete contact");
                Ok(ExecutionResult {
                    success: false,
                    response: format!("❌ Failed to delete contact: {e}"),
                })
            },
        }
    }

    /// Get upcoming birthdays from contacts (used in morning briefing)
    #[allow(dead_code)] // Will be wired in briefing
    pub(super) async fn get_upcoming_birthdays(
        &self,
        days: u32,
    ) -> Option<Vec<crate::ports::ContactSummary>> {
        let contact_service = self.contact_service.as_ref()?;

        match contact_service.get_upcoming_birthdays(days).await {
            Ok(contacts) if !contacts.is_empty() => Some(contacts),
            Ok(_) => None,
            Err(e) => {
                warn!(error = %e, "Failed to fetch upcoming birthdays");
                None
            },
        }
    }
}
