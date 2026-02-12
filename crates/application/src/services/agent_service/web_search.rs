//! Web search handler with LLM-powered summarization

use tracing::info;

use super::{AgentService, ExecutionResult};
use crate::error::ApplicationError;

impl AgentService {
    /// Handle web search command
    ///
    /// Performs a web search and returns results formatted with citations.
    /// Uses the LLM to summarize the search results.
    pub(super) async fn handle_web_search(
        &self,
        query: &str,
        max_results: Option<u32>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref websearch_service) = self.websearch_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "🔍 Web search is not available.\n\n\
                          Web search service is not configured. \
                          Please set up the Brave Search API key in your configuration."
                    .to_string(),
            });
        };

        let max_results = max_results.unwrap_or(5);

        info!(query = %query, max_results = %max_results, "Performing web search");

        let search_response = websearch_service.search_for_llm(query, max_results).await?;

        // If no results, return early
        if search_response.contains("No web search results found") {
            return Ok(ExecutionResult {
                success: true,
                response: format!(
                    "🔍 No results found for: **{query}**\n\n\
                     Try rephrasing your search query or using different keywords."
                ),
            });
        }

        // Use LLM to summarize the search results with proper citation
        let summary_prompt = format!(
            "Based on the following web search results, provide a concise and helpful answer \
             to the query: \"{query}\"\n\n\
             Include relevant information from the sources and cite them using [number] notation \
             at the end of sentences that use information from that source.\n\n\
             Search Results:\n{search_response}\n\n\
             Provide a clear, informative summary with proper source citations."
        );

        let llm_response = self.inference.generate(&summary_prompt).await?;

        Ok(ExecutionResult {
            success: true,
            response: format!(
                "🔍 **Web Search Results for:** {query}\n\n{}\n\n\
                 ---\n*Search powered by {}*",
                llm_response.content,
                websearch_service.provider_name()
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::AgentCommand;

    use super::super::{
        AgentService,
        test_support::{MockInferenceEngine, mock_inference_result},
    };

    #[tokio::test]
    async fn execute_websearch_without_service_returns_error_message() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::WebSearch {
                query: "rust programming".to_string(),
                max_results: None,
            })
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.response.contains("not available"));
        assert!(result.response.contains("not configured"));
    }

    #[tokio::test]
    async fn execute_websearch_with_mock_service() {
        use crate::ports::MockWebSearchPort;

        let mut mock_inference = MockInferenceEngine::new();
        mock_inference.expect_generate().returning(|_| {
            Ok(mock_inference_result(
                "Here is a summary with citations [1][2].",
            ))
        });

        let mut mock_websearch = MockWebSearchPort::new();
        mock_websearch
            .expect_search_for_llm()
            .returning(|query, _| {
                Ok(format!(
                    "[1] Result 1 - example.com: Info about {query}\n\
                     [2] Result 2 - test.org: More info about {query}"
                ))
            });
        mock_websearch
            .expect_provider_name()
            .return_const("mock-provider".to_string());

        let service = AgentService::new(Arc::new(mock_inference))
            .with_websearch_service(Arc::new(mock_websearch));

        let result = service
            .execute_command(&AgentCommand::WebSearch {
                query: "rust async patterns".to_string(),
                max_results: Some(5),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("rust async patterns"));
        assert!(result.response.contains("mock-provider"));
    }

    #[tokio::test]
    async fn execute_websearch_no_results() {
        use crate::ports::MockWebSearchPort;

        let mock_inference = MockInferenceEngine::new();

        let mut mock_websearch = MockWebSearchPort::new();
        mock_websearch
            .expect_search_for_llm()
            .returning(|query, _| Ok(format!("No web search results found for: {query}")));
        mock_websearch
            .expect_provider_name()
            .return_const("mock".to_string());

        let service = AgentService::new(Arc::new(mock_inference))
            .with_websearch_service(Arc::new(mock_websearch));

        let result = service
            .execute_command(&AgentCommand::WebSearch {
                query: "xyznonexistent12345".to_string(),
                max_results: None,
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("No results found"));
    }

    #[tokio::test]
    async fn websearch_service_builder() {
        use crate::ports::MockWebSearchPort;

        let mock = MockInferenceEngine::new();
        let mock_websearch = MockWebSearchPort::new();

        let service =
            AgentService::new(Arc::new(mock)).with_websearch_service(Arc::new(mock_websearch));

        let debug = format!("{service:?}");
        assert!(debug.contains("has_websearch: true"));
    }
}
