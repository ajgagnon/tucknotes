import { FileText } from "lucide-react";

function TranscriptsView() {
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <FileText className="w-12 h-12 text-muted-foreground mb-4" />
      <h1 className="text-xl font-semibold mb-2">Transcripts</h1>
      <p className="text-sm text-muted-foreground">
        Your saved transcripts will appear here.
      </p>
    </div>
  );
}

export default TranscriptsView;
