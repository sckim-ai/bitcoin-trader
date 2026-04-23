-- Add day_psy column to market_data (hour bars only populated; day/week rows stay NULL).
-- NULL = warmup / unavailable. NOT 0 — strategies distinguish these via NaN semantics.

-- SQLite lacks "ADD COLUMN IF NOT EXISTS"; use pragma_table_info guard via DO-block equivalent.
-- Idempotent pattern: ALTER is attempted; if column exists the statement errors and is ignored
-- because each migration runs via execute_batch which stops on the first error. To keep this
-- safe across repeated boot-ups on an already-migrated DB, we use a creation-time default of
-- NULL and rely on the loader to swallow "duplicate column" errors.
ALTER TABLE market_data ADD COLUMN day_psy REAL;
