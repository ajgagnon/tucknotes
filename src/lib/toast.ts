import { toast } from "sonner";

/**
 * Show an error as a bottom-center toast that auto-dismisses. Errors with an
 * action button linger longer so they aren't gone before the user can act.
 */
export function toastError(
  message: string,
  opts?: {
    description?: string;
    action?: { label: string; onClick: () => void };
  },
) {
  return toast.error(message, {
    ...opts,
    duration: opts?.action ? 10000 : 6000,
  });
}
