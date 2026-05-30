import React, { useState, forwardRef } from "react";
import Spinner from "./Spinner";

interface PayPerUseFormProps {
  onPay: (amount: bigint) => Promise<void>;
  loading: boolean;
}

const PayPerUseForm = forwardRef<HTMLInputElement, PayPerUseFormProps>(
  ({ onPay, loading }, ref) => {
    const [amount, setAmount] = useState("");

    async function handleSubmit() {
      if (!amount) return;
      const stroops = BigInt(Math.round(parseFloat(amount) * 10_000_000));
      await onPay(stroops);
      setAmount("");
    }

    return (
      <div className="card">
        <h3 className="ppu-card__title">Pay-per-use</h3>
        <div className="ppu-card__row">
          <input
            ref={ref}
            type="number"
            min="0.0000001"
            step="0.0000001"
            placeholder="Amount in XLM"
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            disabled={loading}
          />
          <button
            onClick={handleSubmit}
            disabled={!amount || loading}
            className="btn-primary ppu-card__pay-btn"
          >
            {loading ? <Spinner size="sm" /> : "Pay now"}
          </button>
        </div>
      </div>
    );
  }
);

PayPerUseForm.displayName = "PayPerUseForm";

export default PayPerUseForm;
