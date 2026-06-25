import React from "react";
import { useFreighterAvailable } from "../hooks/useFreighterAvailable";
import Spinner from "./Spinner";

interface Props {
  onConnect: () => void;
  error: string | null;
  loading?: boolean;
}

export default function ConnectWallet({ onConnect, error, loading = false }: Props) {
  const { available, installUrl } = useFreighterAvailable();

  return (
    <div className="card connect-wallet">
      <p className="connect-wallet__hint">Connect your Freighter wallet to get started.</p>

      {available ? (
        <button onClick={onConnect} className="btn-primary w-full" disabled={loading}>
          {loading ? <Spinner size="sm" /> : "Connect Wallet"}
        </button>
      ) : (
        <a
          href={installUrl}
          target="_blank"
          rel="noopener noreferrer"
          className="btn-primary w-full connect-wallet__install-link"
        >
          Install Freighter
        </a>
      )}

      {error && <p className="text-error">{error}</p>}
    </div>
  );
}
