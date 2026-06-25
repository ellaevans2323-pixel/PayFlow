import React, { useEffect, useState } from "react";
import { getBalance } from "../stellar";
import { formatXlm } from "../utils/format";

interface BalanceDisplayProps {
  address: string;
}

export default function BalanceDisplay({ address }: BalanceDisplayProps) {
  const [balance, setBalance] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let mounted = true;

    async function fetchBalance() {
      setLoading(true);
      try {
        const bal = await getBalance(address);
        if (mounted) {
          setBalance(bal);
        }
      } catch (err) {
        console.error("Failed to fetch balance:", err);
      } finally {
        if (mounted) {
          setLoading(false);
        }
      }
    }

    fetchBalance();

    return () => {
      mounted = false;
    };
  }, [address]);

  if (loading) {
    return <div className="skeleton balance-skeleton" />;
  }

  return (
    <div className="flex flex-col">
      <span className="wallet-bar__label">Balance</span>
      <span className="wallet-bar__address text-mono">
        {balance ? formatXlm(BigInt(Math.floor(Number(balance) * 10_000_000))) : "0.0000000 XLM"}
      </span>
    </div>
  );
}
