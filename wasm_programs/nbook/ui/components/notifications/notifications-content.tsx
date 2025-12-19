"use client"

export function NotificationsContent() {
  return (
    <div className="max-w-2xl mx-auto">
      <div className="sticky top-0 z-40 bg-background/80 backdrop-blur-xl border-b border-border/50 p-4">
        <h1 className="text-2xl font-bold">Notifications</h1>
        <p className="text-sm text-muted-foreground mt-1">
          Notifications are not implemented in the current WASM program.
        </p>
      </div>

      <div className="flex flex-col items-center justify-center py-16 px-4 text-center text-muted-foreground">
        <p className="text-lg text-foreground mb-2">Coming soon</p>
        <p>Once notification support lands in the program, they will appear here.</p>
      </div>
    </div>
  )
}
