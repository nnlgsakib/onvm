"use client"

import { useState } from "react"
import { DashboardView } from "@/components/dashboard-view"
import { StudentsView } from "@/components/students-view"
import { TeachersView } from "@/components/teachers-view"
import { CoursesView } from "@/components/courses-view"
import { GradesView } from "@/components/grades-view"
import { LayoutDashboard, Users, GraduationCap, BookOpen, Award, Menu, X } from "lucide-react"
import { Button } from "@/components/ui/button"

type View = "dashboard" | "students" | "teachers" | "courses" | "grades"

export default function SchoolManagementSystem() {
  const [currentView, setCurrentView] = useState<View>("dashboard")
  const [sidebarOpen, setSidebarOpen] = useState(false)

  const navigation = [
    { id: "dashboard" as View, name: "Dashboard", icon: LayoutDashboard },
    { id: "students" as View, name: "Students", icon: Users },
    { id: "teachers" as View, name: "Teachers", icon: GraduationCap },
    { id: "courses" as View, name: "Courses", icon: BookOpen },
    { id: "grades" as View, name: "Grades", icon: Award },
  ]

  return (
    <div className="flex h-screen bg-background">
      {/* Sidebar */}
      <aside
        className={`fixed inset-y-0 left-0 z-50 w-64 bg-sidebar border-r border-sidebar-border transform transition-transform duration-200 ease-in-out lg:translate-x-0 lg:static ${
          sidebarOpen ? "translate-x-0" : "-translate-x-full"
        }`}
      >
        <div className="flex h-full flex-col">
          {/* Logo */}
          <div className="flex h-16 items-center justify-between px-6 border-b border-sidebar-border">
            <div className="flex items-center gap-2">
              <div className="flex size-8 items-center justify-center rounded-lg bg-primary">
                <GraduationCap className="size-5 text-primary-foreground" />
              </div>
              <span className="text-lg font-semibold text-sidebar-foreground">EduManager</span>
            </div>
            <Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setSidebarOpen(false)}>
              <X className="size-5" />
            </Button>
          </div>

          {/* Navigation */}
          <nav className="flex-1 space-y-1 px-3 py-4">
            {navigation.map((item) => {
              const Icon = item.icon
              const isActive = currentView === item.id
              return (
                <button
                  key={item.id}
                  onClick={() => {
                    setCurrentView(item.id)
                    setSidebarOpen(false)
                  }}
                  className={`flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-sm font-medium transition-colors ${
                    isActive
                      ? "bg-sidebar-accent text-sidebar-accent-foreground"
                      : "text-sidebar-foreground hover:bg-sidebar-accent/50 hover:text-sidebar-accent-foreground"
                  }`}
                >
                  <Icon className="size-5" />
                  {item.name}
                </button>
              )
            })}
          </nav>

          {/* User info */}
          <div className="border-t border-sidebar-border p-4">
            <div className="flex items-center gap-3">
              <div className="flex size-10 items-center justify-center rounded-full bg-primary/10 text-primary font-semibold">
                AD
              </div>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-sidebar-foreground truncate">Admin User</p>
                <p className="text-xs text-muted-foreground truncate">admin@school.edu</p>
              </div>
            </div>
          </div>
        </div>
      </aside>

      {/* Overlay */}
      {sidebarOpen && (
        <div className="fixed inset-0 z-40 bg-black/50 lg:hidden" onClick={() => setSidebarOpen(false)} />
      )}

      {/* Main content */}
      <div className="flex flex-1 flex-col overflow-hidden">
        {/* Header */}
        <header className="flex h-16 items-center gap-4 border-b border-border bg-card px-6">
          <Button variant="ghost" size="icon" className="lg:hidden" onClick={() => setSidebarOpen(true)}>
            <Menu className="size-5" />
          </Button>
          <div className="flex-1">
            <h1 className="text-2xl font-bold text-foreground">{navigation.find((n) => n.id === currentView)?.name}</h1>
          </div>
        </header>

        {/* Content */}
        <main className="flex-1 overflow-y-auto p-6">
          {currentView === "dashboard" && <DashboardView />}
          {currentView === "students" && <StudentsView />}
          {currentView === "teachers" && <TeachersView />}
          {currentView === "courses" && <CoursesView />}
          {currentView === "grades" && <GradesView />}
        </main>
      </div>
    </div>
  )
}
