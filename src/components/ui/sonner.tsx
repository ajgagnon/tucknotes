import { Toaster as Sonner, type ToasterProps } from "sonner";

/** App-wide toast host: bottom-center, themed to match light/dark. */
function Toaster(props: ToasterProps) {
  const theme = document.documentElement.classList.contains("dark")
    ? "dark"
    : "light";

  return (
    <Sonner theme={theme} position="bottom-center" richColors {...props} />
  );
}

export { Toaster };
