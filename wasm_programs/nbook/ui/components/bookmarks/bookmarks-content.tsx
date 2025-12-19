"use client"

import { Bookmark } from "lucide-react"

export function BookmarksContent() {
  return (
    <div className="max-w-2xl mx-auto">
      {/* Header */}
      <div className="sticky top-0 z-40 bg-background/80 backdrop-blur-xl border-b border-border/50 p-4">
        <div className="flex items-center gap-3">
          <Bookmark className="w-6 h-6 text-primary" />
          <div>
            <h1 className="text-2xl font-bold">Bookmarks</h1>
            <p className="text-sm text-muted-foreground">Posts you've saved for later</p>
          </div>
        </div>
      </div>

      <div className="flex flex-col items-center justify-center py-16 px-4 text-center text-muted-foreground">
        <Bookmark className="w-16 h-16 text-muted-foreground mb-4" />
        <h2 className="text-xl font-semibold mb-2 text-foreground">Bookmarks are not supported yet</h2>
        <p>Post bookmarking is not implemented in the current WASM program.</p>
      </div>
    </div>
  )
}
