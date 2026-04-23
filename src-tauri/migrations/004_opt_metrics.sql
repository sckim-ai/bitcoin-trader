-- Per-solution complete metrics blob so the Solutions table can show every
-- objective (not just the ones selected for optimization) — same idea as
-- writing the simulation result summary alongside the Pareto rank.
ALTER TABLE optimization_results ADD COLUMN metrics_json TEXT;
