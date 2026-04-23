-- Speed up the Recent Runs panel:
--   1. `run_id` index → listOptimizationRuns' per-row MAX(total_return)
--      subquery goes from 372k-row full scan to an index seek.
--   2. `(run_id, generation)` composite → getOptimizationRunHistory returns
--      orders-of-magnitude faster when scrubbing through past runs.
CREATE INDEX IF NOT EXISTS idx_opt_results_run_id
    ON optimization_results(run_id);
CREATE INDEX IF NOT EXISTS idx_opt_results_run_gen
    ON optimization_results(run_id, generation);

-- Cache the run's peak return on the runs table so the listing query
-- doesn't need a correlated subquery at all. Updated by the backend on
-- every generation callback (monotonically non-decreasing).
ALTER TABLE optimization_runs ADD COLUMN best_return REAL;
