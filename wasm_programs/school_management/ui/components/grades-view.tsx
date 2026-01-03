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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"
import { getSchoolClient } from "@/lib/onvm-client"
import { Course, Grade, Student } from "@/types"

const PROGRAM_ID = "prog918c95aa44b129ae2315698637afbb339b7cb43ed6115e433605ae172f617bc4"

export function GradesView() {
  const [grades, setGrades] = useState<Grade[]>([])
  const [students, setStudents] = useState<Student[]>([])
  const [courses, setCourses] = useState<Course[]>([])
  const [loading, setLoading] = useState(true)
  const [searchQuery, setSearchQuery] = useState("")
  const [isAddDialogOpen, setIsAddDialogOpen] = useState(false)
  const [formData, setFormData] = useState<Partial<Grade>>({
    date: new Date().toISOString().split("T")[0],
  })
  
  const client = getSchoolClient()
  
  useEffect(() => {
    fetchGrades()
    fetchStudents()
    fetchCourses()
  }, [])
  
  const fetchStudents = async () => {
    try {
      const response = await client.listStudents()
      if (response.status === "success" && response.data) {
        setStudents(response.data)
      }
    } catch (error) {
      console.error("Failed to fetch students:", error)
    }
  }

  const fetchCourses = async () => {
    try {
      const response = await client.listCourses()
      if (response.status === "success" && response.data) {
        setCourses(response.data)
      }
    } catch (error) {
      console.error("Failed to fetch courses:", error)
    }
  }
  
  const fetchGrades = async () => {
    setLoading(true)
    try {
      const allGrades: Grade[] = []
      // Ensure students and courses are loaded before trying to fetch grades
      // This might involve chaining promises or using a single useEffect for all data
      if (students.length === 0) await fetchStudents()
      if (courses.length === 0) await fetchCourses()

      const currentStudents = students.length > 0 ? students : (await client.listStudents()).data || []
      const currentCourses = courses.length > 0 ? courses : (await client.listCourses()).data || []

      for (const student of currentStudents) {
        const response = await client.getStudentGrades(student.id)
        if (response.status === "success" && response.data) {
          const studentGrades = response.data.map((grade: Grade) => {
            const studentInfo = currentStudents.find(s => s.id === grade.student_id)
            const courseInfo = currentCourses.find(c => c.id === grade.course_id)
            return {
              ...grade,
              studentName: studentInfo?.name || "Unknown Student",
              courseName: courseInfo?.name || "Unknown Course",
            }
          })
          allGrades.push(...studentGrades)
        }
      }
      setGrades(allGrades)
    } catch (error) {
      console.error("Failed to fetch grades:", error)
    } finally {
      setLoading(false)
    }
  }
  
  const handleAddGrade = async () => {
    if (formData.student_id && formData.course_id && formData.score) {
      try {
        const newGrade: any = {
          id: Date.now().toString(),
          student_id: formData.student_id,
          course_id: formData.course_id,
          score: formData.score,
          date: formData.date,
        }
        
        const response = await client.assignGrade(newGrade)
        if (response.status === "success") {
          await fetchGrades() // Refresh the list
          setIsAddDialogOpen(false)
          setFormData({ date: new Date().toISOString().split("T")[0] })
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
      grade.studentName?.toLowerCase().includes(searchQuery.toLowerCase()) ||
      grade.courseName?.toLowerCase().includes(searchQuery.toLowerCase()),
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
                        {/* Assuming studentName is available through a join or direct property */}
                        {((grade as any).studentName || "?")
                          .split(" ")
                          .map((n: string) => n[0])
                          .join("")}
                      </div>
                      <span className="text-sm font-medium text-foreground">{(grade as any).studentName}</span>
                    </div>
                  </td>
                  <td className="px-6 py-4 whitespace-nowrap">
                    <span className="text-sm text-foreground">{(grade as any).courseName}</span>
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
              <Label htmlFor="student-id">Student</Label>
              <Select
                onValueChange={(value) => setFormData({ ...formData, student_id: value })}
                value={formData.student_id || ""}
              >
                <SelectTrigger id="student-id">
                  <SelectValue placeholder="Select a student" />
                </SelectTrigger>
                <SelectContent>
                  {students.map((student) => (
                    <SelectItem key={student.id} value={student.id}>
                      {student.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-2">
              <Label htmlFor="course-id">Course</Label>
              <Select
                onValueChange={(value) => setFormData({ ...formData, course_id: value })}
                value={formData.course_id || ""}
              >
                <SelectTrigger id="course-id">
                  <SelectValue placeholder="Select a course" />
                </SelectTrigger>
                <SelectContent>
                  {courses.map((course) => (
                    <SelectItem key={course.id} value={course.id}>
                      {course.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
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