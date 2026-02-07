# Database Migrations

This directory contains SQL migration files for the PiSovereign database schema.

## Migration Format

Migrations follow the naming convention: `VXXX__description.sql`

- `VXXX` - Version number (e.g., V001, V002, V003)
- `__` - Double underscore separator
- `description` - Brief description of the migration
- `.sql` - File extension

## Current Migrations

| Version | File | Description |
|---------|------|-------------|
| V001 | [V001__initial_schema.sql](./V001__initial_schema.sql) | Initial schema with conversations, messages, approvals, audit log |
| V002 | [V002__user_profiles.sql](./V002__user_profiles.sql) | User profiles for preferences and location |
| V003 | [V003__email_drafts.sql](./V003__email_drafts.sql) | Email drafts storage |

## How Migrations Work

1. **Version Tracking**: A `schema_version` table tracks the current database version
2. **Automatic Execution**: Migrations run automatically on startup when `run_migrations = true`
3. **Idempotent**: All statements use `IF NOT EXISTS` to be safely re-runnable
4. **Sequential**: Migrations are applied in version order

## Creating New Migrations

1. Create a new file following the naming convention
2. Use the next version number in sequence
3. All DDL statements should be idempotent (`IF NOT EXISTS`)
4. Update the `SCHEMA_VERSION` constant in `migrations.rs`
5. Add corresponding migration function in code

## Rollback Strategy

Rollbacks are **manual** - if a migration fails:

1. Check the error message for details
2. Fix the underlying issue
3. Manually repair the database if needed
4. Re-run migrations

This approach ensures migrations are explicit and auditable. For critical systems, 
test migrations in a staging environment first.

## Schema Version Table

```sql
CREATE TABLE schema_version (
    version INTEGER PRIMARY KEY
);
```

Query current version:
```sql
SELECT COALESCE(MAX(version), 0) FROM schema_version;
```

## Best Practices

- **Never modify existing migrations** - create a new migration instead
- **Keep migrations small** - one logical change per migration
- **Test on a copy** - verify migrations against a database copy before production
- **Document breaking changes** - clearly note any backwards-incompatible changes
