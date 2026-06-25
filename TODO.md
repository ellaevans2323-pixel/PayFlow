# TODO - MerchantDashboard batch charge

- [x] Step 1: Inspect `contract/src/batch.rs` to confirm the Soroban `batch_charge` entrypoint signature and emitted events.
- [x] Step 2: Implement `buildBatchChargeTx` + `simulateBatchCharge` in `frontend/src/stellar.ts` matching the contract signature.
- [x] Step 3: Update `frontend/src/components/MerchantDashboard.tsx`:

  - [x] Filter due subscribers (`nextChargeAt` in the past)
  - [x] Add “Charge due subscribers” button enabled only when due exist
  - [x] Submit via `useTransaction`
  - [x] After confirmation, parse events from the confirmed tx and show per-subscriber Charged/Skipped/Failed
- [x] Step 4: Update `frontend/src/__tests__/MerchantDashboard.test.tsx` to cover button enabled/disabled and results rendering (mock event parsing + tx submission).
- [x] Step 5: Run frontend tests (`cd frontend && npm test`) and fix any issues.

# TODO - UI Fixes (from Issues.md)

- [x] Issue #1: Show BalanceDisplay in SubscribeForm.tsx (pass userKey as address prop)
- [x] Issue #2: Add CopyButton to WalletBar.tsx (copy full public key to clipboard)
- [x] Issue #3: Render NetworkBadge in WalletBar.tsx (show Testnet/Mainnet)
- [x] Issue #4: Use NextChargeCountdown in SubscriptionCard.tsx (show overdue state)

