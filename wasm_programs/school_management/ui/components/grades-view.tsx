"use client"

import { useState, useEffect } from "react"
import { Button } from "@/components/ui/button"
import { Card } from "@/components/ui/card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Plus, Search } from "lucide-react"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { getSchoolClient } from "@/lib/onvm-client"
import { Grade } from "@/types"

const PROGRAM_ID = "918c95aa44b129ae2315698637afbb339b7cb43ed6115e433605ae172f617bc4"

export function GradesView() {
  const [grades, setGrades] = useState<Grade[]>([])
  const [loading, setLoading] = useState(true)
  const [searchQuery, setSearchQuery] = useState("")
  const [isAddDialogOpen, setIsAddDialogOpen] = useState(false)
  const [formData, setFormData] = useState<Partial<Grade>>({})
  
  const client = getSchoolClient()
  
  useEffect(() => {
    fetchGrades()
  }, [])
  
  const fetchGrades = async () => {
    try {
      setLoading(true)
      // This is a simplification - in a real app, you might need to fetch grades differently
      // For now, we'll just use listStudents as a placeholder
      const response = await client.listStudents()
      if (response.status === "success" && response.data) {
        // This is just placeholder data - you would need to implement proper grade fetching
        setGrades([])
      }
    } catch (error) {
      console.error("Failed to fetch grades:", error)
    } finally {
      setLoading(false)
    }
  }
  
  const handleAddGrade = async () => {
    if (formData.studentId && formData.courseId && formData.score) {
      try {
        const newGrade: any = {
          id: Date.now().toString(),
          student_id: formData.studentId,
          course_id: formData.courseId,
          score: formData.score,
          date: formData.date || new Date().toISOString().split("T")[0],
        }
        
        const response = await client.assignGrade(newGrade)
        if (response.status === "success") {
          await fetchGrades() // Refresh the list
          setIsAddDialogOpen(false)
          setFormData({})
        } else {
          console.error("Failed to assign grade:", response.error)
        }
      } catch (error) {
        console.error("Failed to assign grade:", error)
      }
    }
  }

  const filteredGrades = grades.filter(
    (grade) =>
      (grade as any).student_name?.toLowerCase().includes(searchQuery.toLowerCase()) ||
      (grade as any).course_name?.toLowerCase().includes(searchQuery.toLowerCase()),
  )
  
  if (loading) {
    return <div>Loading grades...</div>
  }

  const getGradeColor = (score: number) => {
    if (score >= 90) return "text-emerald-600 dark:text-emerald-400 bg-emerald-500/10"
    if (score >= 80) return "text-blue-600 dark:text-blue-400 bg-blue-500/10"
    if (score >= 70) return "text-amber-600 dark:text-amber-400 bg-amber-500/10"
    return "text-red-600 dark:text-red-400 bg-red-500/10"
  }

  const getGradeLetter = (score: number) => {
    if (score >= 90) return "A"
    if (score >= 80) return "B"
    if (score >= 70) return "C"
    if (score >= 60) return "D"
    return "F"
  }

  return (
    <div className="space-y-6">
      {/* Header Actions */}
      <div className="flex flex-col sm:flex-row gap-4 items-start sm:items-center justify-between">
        <div className="relative flex-1 max-w-md w-full">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 size-4 text-muted-foreground" />
          <Input
            placeholder="Search grades..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9"
          />
        </div>
        <Button onClick={() => setIsAddDialogOpen(true)} className="w-full sm:w-auto">
          <Plus className="size-4 mr-2" />
          Assign Grade
        </Button>
      </div>

      {/* Grades Table */}
      <Card className="overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full">
            <thead className="bg-muted/50 border-b border-border">
              <tr>
                <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  Student
                </th>
                <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  Course
                </th>
                <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  Score
                </th>
                <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  Grade
                </th>
                <th className="px-6 py-3 text-left text-xs font-medium text-muted-foreground uppercase tracking-wider">
                  Date
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {filteredGrades.map((grade) => (
                <tr key={grade.id} className="hover:bg-muted/30 transition-colors">
                  <td className="px-6 py-4 whitespace-nowrap">
                    <div className="flex items-center gap-3">
                      <div className="flex size-8 items-center justify-center rounded-full bg-primary/10 text-primary font-semibold text-xs">
                        {grade.studentName
                          .split(" ")
                          .map((n) => n[0])
                          .join("")}
                      </div>
                      <span className="text-sm font-medium text-foreground">{grade.studentName}</span>
                    </div>
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap">
                    <span className="text-sm text-foreground">{grade.courseName}</span>
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap">
                    <span className="text-sm font-semibold text-foreground">{grade.score}%</span>
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap">
                    <span
                      className={`inline-flex items-center px-2.5 py-1 rounded-full text-xs font-semibold ${getGradeColor(grade.score)}`}
                    >
                      {getGradeLetter(grade.score)}
                    </span>
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap">
                    <span className="text-sm text-muted-foreground">{new Date(grade.date).toLocaleDateString()}</span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Card>

      {/* Add Grade Dialog */}
      <Dialog open={isAddDialogOpen} onOpenChange={setIsAddDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>Assign Grade</DialogTitle>
            <DialogDescription>Enter the grade information below to assign it to a student.</DialogDescription>
          </DialogHeader>
          <div className="space-y-4 py-4">
            <div className="space-y-2">
              <Label htmlFor="student">Student Name</Label>
              <Input
                id="student"
                placeholder="Emma Wilson"
                value={formData.studentName || ""}
                onChange={(e) => setFormData({ ...formData, studentName: e.target.value })}
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="course">Course Name</Label>
              <Input
                id="course"
                placeholder="Advanced Mathematics"
                value={formData.courseName || ""}
                onChange={(e) => setFormData({ ...formData, courseName: e.target.value })}
              />
            </div>
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="score">Score (%)</Label>
                <Input
                  id="score"
                  type="number"
                  min="0"
                  max="100"
                  placeholder="95"
                  value={formData.score || ""}
                  onChange={(e) => setFormData({ ...formData, score: Number.parseInt(e.target.value) })}
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="date">Date</Label>
                <Input
                  id="date"
                  type="date"
                  value={formData.date || new Date().toISOString().split("T")[0]}
                  onChange={(e) => setFormData({ ...formData, date: e.target.value })}
                />
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setIsAddDialogOpen(false)}>
              Cancel
            </Button>
            <Button onClick={handleAddGrade}>Assign Grade</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}