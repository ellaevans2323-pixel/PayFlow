import { useState, useCallback, useRef, useEffect } from "react";
import { StrKey } from "@stellar/stellar-sdk";
import { server } from "../stellar";
import { CONTRACT_LIMITS } from "../constants";

export interface ValidationResult {
  valid: boolean;
  error?: string;
}

export interface FormFields {
  merchant: string;
  amount: string;
  interval: number;
}

export interface FormErrors {
  merchant?: string;
  amount?: string;
  interval?: string;
}

interface UseFormValidationResult {
  errors: FormErrors;
  validate: (fields: FormFields) => boolean;
  isValid: boolean;
  validating: boolean;
  validateAsync: (fields: FormFields) => Promise<boolean>;
}

export function validateStroopAmount(value: string, maxStroops: bigint): ValidationResult {
  const num = parseFloat(value);
  if (!value || isNaN(num) || num <= 0) {
    return { valid: false, error: "Amount must be greater than 0." };
  }
  const stroops = BigInt(Math.round(num * 10_000_000));
  if (stroops > maxStroops) {
    return { valid: false, error: `Amount exceeds maximum of ${maxStroops} stroops.` };
  }
  return { valid: true };
}

export function validateInterval(seconds: number, minSeconds: number): ValidationResult {
  if (!seconds || seconds <= 0) {
    return { valid: false, error: "Interval must be greater than 0." };
  }
  if (seconds < minSeconds) {
    return { valid: false, error: `Interval must be at least ${minSeconds} seconds.` };
  }
  return { valid: true };
}

export function validateAddress(addr: string): ValidationResult {
  if (!addr) {
    return { valid: false, error: "Address is required." };
  }
  if (!StrKey.isValidEd25519PublicKey(addr)) {
    return { valid: false, error: "Invalid Stellar address." };
  }
  return { valid: true };
}

export function useFormValidation(): UseFormValidationResult {
  const [errors, setErrors] = useState<FormErrors>({});
  const [validating, setValidating] = useState(false);
  const abortControllerRef = useRef<AbortController | null>(null);

  const validate = useCallback((fields: FormFields): boolean => {
    const next: FormErrors = {};

    const addressResult = validateAddress(fields.merchant);
    if (!addressResult.valid) {
      next.merchant = addressResult.error;
    }

    const amountResult = validateStroopAmount(fields.amount, CONTRACT_LIMITS.MAX_SUBSCRIPTION_AMOUNT);
    if (!amountResult.valid) {
      next.amount = amountResult.error;
    }

    const intervalResult = validateInterval(fields.interval, CONTRACT_LIMITS.MIN_INTERVAL_SECONDS);
    if (!intervalResult.valid) {
      next.interval = intervalResult.error;
    }

    setErrors(next);
    return Object.keys(next).length === 0;
  }, []);

  const validateAsync = useCallback(async (fields: FormFields): Promise<boolean> => {
    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }

    const syncPassed = validate(fields);
    if (!syncPassed) {
      return false;
    }

    const controller = new AbortController();
    abortControllerRef.current = controller;

    setValidating(true);
    try {
      await server.getAccount(fields.merchant);

      if (controller.signal.aborted) {
        return false;
      }

      setErrors(prev => {
        if (prev.merchant === "Account not found on network.") {
          const { merchant: _, ...rest } = prev;
          return rest;
        }
        return prev;
      });

      return true;
    } catch (err) {
      if (controller.signal.aborted) {
        return false;
      }

      setErrors(prev => ({
        ...prev,
        merchant: "Account not found on network.",
      }));

      return false;
    } finally {
      if (!controller.signal.aborted) {
        setValidating(false);
      }
    }
  }, [validate]);

  useEffect(() => {
    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, []);

  return {
    errors,
    validate,
    isValid: Object.keys(errors).length === 0,
    validating,
    validateAsync,
  };
}

