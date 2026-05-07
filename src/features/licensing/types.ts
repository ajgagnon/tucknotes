export type LicenseStatus =
  | { kind: "Trial"; days_remaining: number }
  | { kind: "TrialExpired" }
  | {
      kind: "Licensed";
      last_validated_at: number;
      expires_grace_at: number;
    }
  | { kind: "LicenseInvalid"; reason: string };

export function allowsPaidFeatures(status: LicenseStatus | null): boolean {
  if (!status) return false;
  return status.kind === "Trial" || status.kind === "Licensed";
}

/// Polar-hosted checkout URL.
export const BUY_URL =
  "https://buy.polar.sh/polar_cl_HWYPiN7THeqkfcU3CxsPZg3Def8vO9lE1SsBL1Q2Vys";
