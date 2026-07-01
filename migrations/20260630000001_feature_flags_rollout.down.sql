-- Rollback: Remove feature flags rollout and targeting columns

DROP INDEX IF EXISTS idx_feature_flags_rollout_pct;
DROP INDEX IF EXISTS idx_feature_flags_targets;

ALTER TABLE feature_flags DROP COLUMN IF EXISTS rollout_percentage;
ALTER TABLE feature_flags DROP COLUMN IF EXISTS target_contract_ids;
ALTER TABLE feature_flags DROP COLUMN IF EXISTS target_user_ids;
ALTER TABLE feature_flags DROP COLUMN IF EXISTS target_ips;
ALTER TABLE feature_flags DROP COLUMN IF EXISTS target_regions;
