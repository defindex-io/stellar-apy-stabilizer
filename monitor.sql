-- monitor.sql — Live APY + manager status for vaults under the FeeProxy bot,
-- plus the dedicated control vault for side-by-side comparison.
--
-- Returns gross/net APY (7d, 30d, all-time), TVL-weighted strategy APY, and
-- current manager for:
--   - every vault whose current manager is the FeeProxy contract, AND
--   - the control vault (managed by a different address; used to baseline what
--     a comparable vault looks like without the stabilizer bot running).
--
-- Run against the defindex-indexer database.

SELECT
  v.vault_id,
  m.new_address                AS manager,
  v.apy_7d                     AS gross_apy_7d,
  v.apy_7d_net                 AS net_apy_7d,
  v.apy_30d                    AS gross_apy_30d,
  v.apy_30d_net                AS net_apy_30d,
  v.apy_all_time               AS gross_apy_all_time,
  v.apy_all_time_net           AS net_apy_all_time,
  v.strategy_apy_7d,
  v.strategy_apy_30d,
  v.tvl,
  v.total_supply
FROM parsed.v_vault_apy v
JOIN (
  SELECT DISTINCT ON (vault_id) vault_id, new_address
  FROM parsed.vault_role_change
  WHERE role_type = 'manager'
  ORDER BY vault_id, ledger DESC
) m ON m.vault_id = v.vault_id
WHERE m.new_address = 'CDEFLWJMPR6DDNOEGP6KNPSPRWKPUG3DJLIOQZIS6EHIGNK7EGTQSA7R'
   OR v.vault_id   = 'CB5YXWIDBQAOTTPEQE3SRNUFM2PTOXFHKGUWCBJJSF2GPW37DN725FDA'
ORDER BY v.vault_id;
