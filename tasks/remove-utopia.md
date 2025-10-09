Remove Swagger/utoipa documentation system

Removed the Swagger UI and utoipa OpenAPI documentation system as it was
not being maintained and showing incorrect information (FOSS-only endpoints
even in SaaS mode).

Changes:
- Remove utoipa and utoipa-swagger-ui dependencies from Cargo.toml
- Remove all utoipa annotations (ToSchema, #[utoipa::path], #[schema])
  from 9 backend source files
- Remove Swagger UI route (/swagger-ui) from API router
- Remove 3 Swagger-specific tests from stripe_integration_tests.rs
- Remove /swagger-ui reference from CLAUDE.md

Impact:
- Removed ~600 lines of code
- All API endpoints continue to work identically
- All tests pass (57 unit/integration tests)
- No functional changes to the application