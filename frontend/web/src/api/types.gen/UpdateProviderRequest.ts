export type UpdateProviderRequest = {
  kind: string;
  base_url: string;
  api_key_env: string;
  api_key?: string | null;
};
