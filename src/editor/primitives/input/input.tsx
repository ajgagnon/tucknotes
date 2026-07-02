import { cn } from "@/editor/tiptap-utils";
import "@/editor/primitives/input/input.scss";

function Input({ className, type, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      data-slot="tiptap-input"
      className={cn("tiptap-input", className)}
      {...props}
    />
  );
}

export { Input };
