# TODO - MerchantDashboard batch charge

- [x] Step 1: Inspect `contract/src/batch.rs` to confirm the Soroban `batch_charge` entrypoint signature and emitted events.
- [x] Step 2: Implement `buildBatchChargeTx` + `simulateBatchCharge` in `frontend/src/stellar.ts` matching the contract signature.
- [ ] Step 3: Update `frontend/src/components/MerchantDashboard.tsx`:

  - [ ] Filter due subscribers (`nextChargeAt` in the past)
  - [ ] Add “Charge due subscribers” button enabled only when due exist
  - [ ] Submit via `useTransaction`
  - [ ] After confirmation, parse events from the confirmed tx and show per-subscriber Charged/Skipped/Failed
- [ ] Step 4: Update `frontend/src/__tests__/MerchantDashboard.test.tsx` to cover button enabled/disabled and results rendering (mock event parsing + tx submission).
- [ ] Step 5: Run frontend tests (`cd frontend && npm test`) and fix any issues.

