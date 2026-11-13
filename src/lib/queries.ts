import { useQuery } from "@tanstack/react-query";
import { api } from "./api";
import { qk } from "./queryKeys";
import type { AccountRow, CategoryRow, UserRow } from "./types";

export const useUsers = () =>
  useQuery({ queryKey: qk.entity("users"), queryFn: () => api.entityList<UserRow>("users") });
export const useAccounts = (includeDeleted = false) =>
  useQuery({ queryKey: qk.entity("accounts", includeDeleted), queryFn: () => api.entityList<AccountRow>("accounts", includeDeleted) });
export const useCategories = (includeDeleted = false) =>
  useQuery({ queryKey: qk.entity("categories", includeDeleted), queryFn: () => api.entityList<CategoryRow>("categories", includeDeleted) });
export const useSettings = () => useQuery({ queryKey: qk.settings, queryFn: api.getSettings });
