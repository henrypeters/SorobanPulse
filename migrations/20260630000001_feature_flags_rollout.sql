-- Issue #632: Add percentage-based rollout and targeting for feature flags

ALTER TABLE feature_flags ADD COLUMN IF NOT EXISTS rollout_percentage INT DEFAULT 100;
ALTER TABLE feature_flags ADD COLUMN IF NOT EXISTS target_contract_ids TEXT ARRAY DEFAULT NULL;
ALTER TABLE feature_flags ADD COLUMN IF NOT EXISTS target_user_ids TEXT ARRAY DEFAULT NULL;
ALTER TABLE feature_flags ADD COLUMN IF NOT EXISTS target_ips TEXT ARRAY DEFAULT NULL;
ALTER TABLE feature_flags ADD COLUMN IF NOT EXISTS target_regions TEXT ARRAY DEFAULT NULL;

-- Create index for faster targeting lookups
CREATE INDEX IF NOT EXISTS idx_feature_flags_rollout_pct ON feature_flags(rollout_percentage) WHERE enabled = TRUE;
CREATE INDEX IF NOT EXISTS idx_feature_flags_targets ON feature_flags USING GIN(target_contract_ids, target_user_ids) WHERE enabled = TRUE;

-- Verify that existing flags have rollout_percentage set to 100 (fully enabled)
UPDATE feature_flags SET rollout_percentage = 100 WHERE rollout_percentage IS NULL;
