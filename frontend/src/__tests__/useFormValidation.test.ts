import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { useFormValidation, type FormFields } from "../hooks/useFormValidation";

const validFields: FormFields = {
  merchant: "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
  amount: "10",
  interval: 30,
};

function validateFields(fields: FormFields) {
  const hook = renderHook(() => useFormValidation());
  let isValid = false;

  act(() => {
    isValid = hook.result.current.validate(fields);
  });

  return { ...hook, isValid };
}

describe("useFormValidation", () => {
  it("returns a merchant error when merchant address is empty", () => {
    const { result, isValid } = validateFields({
      ...validFields,
      merchant: "",
    });

    expect(isValid).toBe(false);
    expect(result.current.errors.merchant).toBe("Merchant address is required.");
  });

  it("returns a merchant error when merchant address is not a valid Stellar address", () => {
    const { result, isValid } = validateFields({
      ...validFields,
      merchant: "not-a-stellar-address",
    });

    expect(isValid).toBe(false);
    expect(result.current.errors.merchant).toBe("Invalid Stellar address.");
  });

  it("returns an amount error when amount is zero", () => {
    const { result, isValid } = validateFields({
      ...validFields,
      amount: "0",
    });

    expect(isValid).toBe(false);
    expect(result.current.errors.amount).toBe("Amount must be greater than 0.");
  });

  it("returns no errors for valid fields", () => {
    const { result, isValid } = validateFields(validFields);

    expect(isValid).toBe(true);
    expect(result.current.errors).toEqual({});
  });
});
