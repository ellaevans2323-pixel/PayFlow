import React from "react";
import { formatAddress } from "../utils/format";
import CopyButton from "./CopyButton";
import NetworkBadge from "./NetworkBadge";
import BalanceDisplay from "./BalanceDisplay";

interface WalletBarProps {
  publicKey: string;
  onDisconnect: () => void;
}

export default function WalletBar({
  publicKey,
  onDisconnect,
}: WalletBarProps) {
  return (
    <div className="card wallet-bar">
      <div className="wallet-bar__content">
        <div className="wallet-bar__connection">
          <span className="wallet-bar__label">Connected</span>
          <div className="wallet-bar__address-row">
            <span className="wallet-bar__address">
              {formatAddress(publicKey)}
            </span>
            <CopyButton text={publicKey} ariaLabel="Copy wallet address" />
          </div>
        </div>
        <BalanceDisplay address={publicKey} />
        <NetworkBadge />
      </div>
      <button onClick={onDisconnect} className="btn-secondary">
        Disconnect
      </button>
    </div>
  );
}
