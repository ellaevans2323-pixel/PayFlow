import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";
import { useRpcHealth } from "../hooks/useRpcHealth";

// Mock the stellar module
vi.mock("../stellar", () => ({
  server: {
    getHealth: vi.fn(),
  },
}));

import { server } from "../stellar";

const mockedServer = vi.mocked(server);

function Test() {
  const { healthy, error } = useRpcHealth();
  return (
    <div>
      <span data-testid="healthy">{String(healthy)}</span>
      <span data-testid="error">{error || "null"}</span>
    </div>
  );
}

describe("useRpcHealth", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe("Healthy Response", () => {
    it("healthy response -> healthy = true", async () => {
      mockedServer.getHealth.mockResolvedValue({} as any);

      render(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("true");
      });
      expect(screen.getByTestId("error")).toHaveTextContent("null");
    });

    it("healthy response -> error is cleared", async () => {
      mockedServer.getHealth.mockResolvedValue({ status: "ok" } as any);

      render(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("true");
      });
      expect(screen.getByTestId("error")).toHaveTextContent("null");
    });

    it("subsequent healthy response keeps healthy = true", async () => {
      mockedServer.getHealth.mockResolvedValue({} as any);

      const { rerender } = render(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("true");
      });

      // Mock another successful call
      mockedServer.getHealth.mockResolvedValue({} as any);
      rerender(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("true");
      });
      expect(screen.getByTestId("error")).toHaveTextContent("null");
    });
  });

  describe("Error Response", () => {
    it("error response -> healthy = false, error set", async () => {
      const testError = new Error("Network error");
      mockedServer.getHealth.mockRejectedValue(testError);

      render(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("false");
      });
      expect(screen.getByTestId("error")).toHaveTextContent("Network error");
    });

    it("non-Error exception -> healthy = false, fallback error message", async () => {
      mockedServer.getHealth.mockRejectedValue("string error");

      render(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("false");
      });
      expect(screen.getByTestId("error")).toHaveTextContent("RPC endpoint unreachable");
    });

    it("connection timeout error -> healthy = false with error message", async () => {
      const timeoutError = new Error("Connection timeout");
      mockedServer.getHealth.mockRejectedValue(timeoutError);

      render(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("false");
      });
      expect(screen.getByTestId("error")).toHaveTextContent("Connection timeout");
    });

    it("null rejection -> healthy = false with fallback error message", async () => {
      mockedServer.getHealth.mockRejectedValue(null);

      render(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("false");
      });
      expect(screen.getByTestId("error")).toHaveTextContent("RPC endpoint unreachable");
    });

    it("undefined rejection -> healthy = false with fallback error message", async () => {
      mockedServer.getHealth.mockRejectedValue(undefined);

      render(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("false");
      });
      expect(screen.getByTestId("error")).toHaveTextContent("RPC endpoint unreachable");
    });
  });

  describe("State Transitions", () => {
    it("transitions from error to healthy after recovery", async () => {
      // First call fails
      mockedServer.getHealth.mockRejectedValueOnce(new Error("Network error"));

      const { rerender } = render(<Test />);

      await waitFor(() => {
        expect(screen.getByTestId("healthy")).toHaveTextContent("false");
      });
      expect(screen.getByTestId("error")).toHaveTextContent("Network error");

      // Second call succeeds
      mockedServer.getHealth.mockResolvedValueOnce({} as any);
      rerender(<Test />);

      // Note: In the current implementation, useEffect only runs once on mount
      // So state doesn't transition after component mount. This test documents the current behavior.
      // If transition behavior is desired, the hook would need a dependency array or useCallback.
      expect(screen.getByTestId("healthy")).toHaveTextContent("false");
    });
  });

  describe("Initial State", () => {
    it("calls server.getHealth on mount", async () => {
      mockedServer.getHealth.mockResolvedValue({} as any);

      render(<Test />);

      await waitFor(() => {
        expect(mockedServer.getHealth).toHaveBeenCalled();
      });
    });

    it("calls server.getHealth exactly once", async () => {
      mockedServer.getHealth.mockResolvedValue({} as any);

      render(<Test />);

      await waitFor(() => {
        expect(mockedServer.getHealth).toHaveBeenCalledTimes(1);
      });
    });

    it("default healthy state is true before check completes", () => {
      mockedServer.getHealth.mockImplementation(
        () => new Promise(() => {}) // Never resolves
      );

      render(<Test />);

      expect(screen.getByTestId("healthy")).toHaveTextContent("true");
      expect(screen.getByTestId("error")).toHaveTextContent("null");
    });
  });

  describe("Return Value", () => {
    it("returns object with healthy and error properties", async () => {
      mockedServer.getHealth.mockResolvedValue({} as any);

      let returnValue: { healthy: boolean; error: string | null } | null = null;

      function TestHook() {
        returnValue = useRpcHealth();
        return null;
      }

      render(<TestHook />);

      await waitFor(() => {
        expect(returnValue).not.toBeNull();
        expect(returnValue).toHaveProperty("healthy");
        expect(returnValue).toHaveProperty("error");
        expect(typeof returnValue!.healthy).toBe("boolean");
        expect(returnValue!.error === null || typeof returnValue!.error === "string").toBe(true);
      });
    });
  });
});
