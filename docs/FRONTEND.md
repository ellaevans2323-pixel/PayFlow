# Frontend Architecture Guide

This document provides a comprehensive overview of the frontend architecture for the PayFlow application. It outlines the technical stack, core Stellar integration patterns, custom React hooks composition, wallet connection lifecycles, global state management strategies, and the structural design of the interface components.

---

## 1. Technical Stack Overview

The PayFlow frontend is engineered to provide a high-performance, resilient, and completely type-safe user interface for interacting with smart contracts on the Stellar network.

* **Framework:** Next.js (App Router) for hybrid server-side rendering, optimized page delivery, and routing.
* **Language:** TypeScript enforced with strict type-safety boundaries across all API endpoints and network interactions.
* **Styling:** Tailwind CSS using a structured component layout system for responsive utility-first design.
* **Blockchain Integration:** `@stellar/stellar-sdk` for transaction construction, XDR parsing, and Horizon RPC communication.
* **Wallet Interoperability:** `@stellar/freighter-api` as the primary non-custodial browser wallet connector.

---

## 2. stellar.ts Core Architecture

All low-level interaction with the Stellar ledger is centralized within `lib/stellar.ts`. This decoupling ensures that components never trigger untyped or raw RPC commands directly. Operations are strictly segmented into **Read Functions** and **Write Functions**.

### Read Functions (Queries)
Read functions query the current state of a Stellar smart contract or account without modifying data or consuming network fees.
* **Mechanism:** They instantiate a `SorobanRpc.Server` connection and invoke the `simulateTransaction` method using a structural mock transaction.
* **Characteristics:** Synchronous behavior, instantaneous execution, requires no user signatures, and returns decoded XDR data directly.

### Write Functions (Transactions)
Write functions submit state-changing transactions to the Stellar network, which alters contract storage slots and burns lumens (`XLM`) for transaction fees.
* **Mechanism:** They construct an explicit transaction envelope via the `TransactionBuilder`, fetch the current account sequence number, append the necessary contract invocation arguments, and request an external cryptographic signature.
* **Characteristics:** Asynchronous multi-stage processing, requires explicit user authorization via a connected wallet, requires gas/fee estimation, and relies on checking status hashes until a validated ledger block is forged.

---

## 3. Hook Composition Patterns

Components never consume contract methods or state directly from `stellar.ts`. Instead, PayFlow utilizes a layered **Hook Composition Pattern** that abstracts data fetching, loading flags, error boundaries, and state caching into modular React Hooks.

### Architectural Blueprint
1.  **Low-Level API Layer (`lib/stellar.ts`):** Handles raw transaction syntax and network serialization.
2.  **Mid-Level Custom Hook Layer (`hooks/useContract.ts`):** Manages asynchronous life cycles, handles execution exceptions, and controls loading indicators.
3.  **UI View Layer (`components/`):** Consumes reactive data values and hooks up operational handlers directly to interactive inputs.

### Example Implementation

```typescript
import { useState, useEffect, useCallback } from 'react';
import { get_fee } from '@/lib/stellar';

export interface FeeConfig {
  collector: string;
  feeBps: number;
}

/**
 * Mid-Level Custom Hook managing fee state configuration fetching
 */
export function useFeeCollector() {
  const [data, setData] = useState<FeeConfig | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<Error | null>(null);

  const fetchFeeConfig = useCallback(async () => {
    try {
      setIsLoading(true);
      setError(null);
      const config = await get_fee();
      setData({
        collector: config.collector,
        feeBps: config.fee_bps,
      });
    } catch (err) {
      setError(err instanceof Error ? err : new Error('Failed to fetch fee configuration'));
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchFeeConfig();
  }, [fetchFeeConfig]);

  return { data, isLoading, error, refetch: fetchFeeConfig };
}
