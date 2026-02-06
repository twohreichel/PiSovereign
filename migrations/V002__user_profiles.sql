-- Migration V002: User Profiles
-- Adds user profiles table for storing preferences and location

CREATE TABLE IF NOT EXISTS user_profiles (
    user_id TEXT PRIMARY KEY,
    latitude REAL,
    longitude REAL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Index for timezone queries
CREATE INDEX IF NOT EXISTS idx_user_profiles_timezone ON user_profiles(timezone);
