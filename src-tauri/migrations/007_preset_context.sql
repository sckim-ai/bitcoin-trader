-- Phase 1.5: presets now carry the full simulation context
-- (market, timeframe, since/until) so a user can re-verify a saved config.

ALTER TABLE presets ADD COLUMN market TEXT;
ALTER TABLE presets ADD COLUMN timeframe TEXT;
ALTER TABLE presets ADD COLUMN since_ts TEXT;
ALTER TABLE presets ADD COLUMN until_ts TEXT;
