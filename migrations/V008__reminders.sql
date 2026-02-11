-- Migration V008: Reminders - Proactive notification system
-- Creates table for reminder storage with lifecycle management

-- Main reminders table
CREATE TABLE IF NOT EXISTS reminders (
    -- UUID primary key
    id TEXT PRIMARY KEY,
    -- User who owns this reminder
    user_id TEXT NOT NULL,
    -- Source type: 'calendar_event', 'calendar_task', 'custom'
    source TEXT NOT NULL CHECK(source IN ('calendar_event', 'calendar_task', 'custom')),
    -- External source ID for deduplication (CalDAV UID, etc.)
    source_id TEXT,
    -- Short title/summary
    title TEXT NOT NULL,
    -- Optional detailed description
    description TEXT,
    -- When the actual event/task occurs (ISO 8601)
    event_time TEXT,
    -- When the reminder notification should fire (ISO 8601)
    remind_at TEXT NOT NULL,
    -- Event location (free-form address)
    location TEXT,
    -- Lifecycle status: 'pending', 'sent', 'acknowledged', 'snoozed', 'cancelled', 'expired'
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK(status IN ('pending', 'sent', 'acknowledged', 'snoozed', 'cancelled', 'expired')),
    -- Snooze tracking
    snooze_count INTEGER NOT NULL DEFAULT 0,
    max_snooze INTEGER NOT NULL DEFAULT 3,
    -- Timestamps (ISO 8601)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Index: Due reminders query (most frequent scheduled query)
CREATE INDEX IF NOT EXISTS idx_reminders_due
    ON reminders(remind_at, status)
    WHERE status IN ('pending', 'snoozed');

-- Index: User's active reminders
CREATE INDEX IF NOT EXISTS idx_reminders_user_active
    ON reminders(user_id, status)
    WHERE status IN ('pending', 'sent', 'snoozed');

-- Index: Deduplication by source
CREATE INDEX IF NOT EXISTS idx_reminders_source
    ON reminders(source, source_id)
    WHERE source_id IS NOT NULL;

-- Index: Cleanup of old terminal reminders
CREATE INDEX IF NOT EXISTS idx_reminders_cleanup
    ON reminders(updated_at, status)
    WHERE status IN ('acknowledged', 'cancelled', 'expired');

-- Index: User reminder count
CREATE INDEX IF NOT EXISTS idx_reminders_user
    ON reminders(user_id);
