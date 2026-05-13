// Hook that fetches per-provider cost history snapshots from the
// Rust core and re-fetches when the popup becomes visible.
//
// Returns a map keyed by provider id. The hook never throws — on
// error we surface an empty map plus an `error` string so the chart
// can render its empty state instead of unmounting.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface ModelCost {
  model_id: string;
  cost_usd: number;
  total_tokens: number;
}

export interface DailyCostEntry {
  date: string; // YYYY-MM-DD
  cost_usd: number;
  total_tokens: number;
  models: ModelCost[];
}

export interface ProviderCostSnapshot {
  current_cycle_usd: number;
  previous_cycle_usd: number | null;
  last_30_days_usd: number[];
  daily: DailyCostEntry[];
  total_window_usd: number;
  updated_at_unix_secs: number;
  breakdown_by_service: Array<{ service_name: string; current_cycle_usd: number }>;
}

interface State {
  byProvider: Record<string, ProviderCostSnapshot>;
  loading: boolean;
  error: string | null;
}

const EMPTY_STATE: State = {
  byProvider: {},
  loading: false,
  error: null,
};

export function useCostHistory() {
  const [state, setState] = useState<State>({ ...EMPTY_STATE, loading: true });

  const fetchOnce = useCallback(async () => {
    setState((s) => ({ ...s, loading: true, error: null }));
    try {
      const data = await invoke<Record<string, ProviderCostSnapshot>>(
        "cost_snapshots",
      );
      setState({ byProvider: data ?? {}, loading: false, error: null });
    } catch (e) {
      setState({
        byProvider: {},
        loading: false,
        error: String(e),
      });
    }
  }, []);

  useEffect(() => {
    void fetchOnce();
  }, [fetchOnce]);

  const refresh = useCallback(async () => {
    try {
      await invoke("refresh_cost_history");
    } catch {
      /* swallow */
    }
    await fetchOnce();
  }, [fetchOnce]);

  return { ...state, refresh };
}
