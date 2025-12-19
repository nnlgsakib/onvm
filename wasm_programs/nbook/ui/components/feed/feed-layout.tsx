"use client"

import type React from "react"

import { Sidebar } from "./sidebar"
import { RightPanel } from "./right-panel"
import { MobileNav } from "./mobile-nav"

export function FeedLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-screen bg-gradient-to-br from-background via-secondary/20 to-background">
      {/* Desktop Layout */}
      <div className="hidden lg:flex">
        {/* Left Sidebar */}
        <div className="fixed top-0 left-0 h-screen w-64 xl:w-72 border-r border-border/40 bg-card/50 backdrop-blur-xl shadow-lg">
          <Sidebar />
        </div>

        {/* Main Content */}
        <main className="flex-1 ml-64 xl:ml-72 mr-0 xl:mr-80 min-h-screen px-4">{children}</main>

        {/* Right Panel */}
        <div className="hidden xl:block fixed top-0 right-0 h-screen w-80 border-l border-border/40 bg-card/60 backdrop-blur-xl overflow-y-auto shadow-lg">
          <RightPanel />
        </div>
      </div>

      {/* Mobile/Tablet Layout */}
      <div className="lg:hidden">
        <main className="pb-20 px-2">{children}</main>
        <MobileNav />
      </div>
    </div>
  )
}
