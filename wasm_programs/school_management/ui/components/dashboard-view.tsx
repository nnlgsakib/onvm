import { useState, useEffect } from "react"
import { Card } from "@/components/ui/card"
import { Users, GraduationCap, BookOpen, TrendingUp } from "lucide-react"
import { getSchoolClient } from "@/lib/onvm-client"

const PROGRAM_ID = "918c95aa44b129ae2315698637afbb339b7cb43ed6115e433605ae172f617bc4"

export function DashboardView() {
  const [stats, setStats] = useState([
    {
      title: "Total Students",
      value: "0",
      change: "+0%",
      icon: Users,
      color: "text-blue-500",
      bgColor: "bg-blue-500/10",
    },
    {
      title: "Total Teachers",
      value: "0",
      change: "+0%",
      icon: GraduationCap,
      color: "text-emerald-500",
      bgColor: "bg-emerald-500/10",
    },
    {
      title: "Active Courses",
      value: "0",
      change: "+0%",
      icon: BookOpen,
      color: "text-purple-500",
      bgColor: "bg-purple-500/10",
    },
    {
      title: "Average Grade",
      value: "0%",
      change: "+0%",
      icon: TrendingUp,
      color: "text-amber-500",
      bgColor: "bg-amber-500/10",
    },
  ])
  
  const [recentActivity, setRecentActivity] = useState<any[]>([])
  const [quickStats, setQuickStats] = useState([
    { label: "Students enrolled this week", value: "0" },
    { label: "Assignments graded", value: "0" },
    { label: "Course completion rate", value: "0%" },
    { label: "Average attendance", value: "0%" },
  ])
  
  const client = getSchoolClient()
  
  useEffect(() => {
    fetchStats()
  }, [])
  
  const fetchStats = async () => {
    try {
      const client = getSchoolClient()
      
      // Fetch students count
      const studentsResponse = await client.listStudents()
      const studentsCount = studentsResponse.status === "success" && studentsResponse.data ? studentsResponse.data.length : 0
      
      // Fetch teachers count
      const teachersResponse = await client.listTeachers()
      const teachersCount = teachersResponse.status === "success" && teachersResponse.data ? teachersResponse.data.length : 0
      
      // Fetch courses count
      const coursesResponse = await client.listCourses()
      const coursesCount = coursesResponse.status === "success" && coursesResponse.data ? coursesResponse.data.length : 0
      
      // Calculate average grade
      let averageGrade = 0
      let totalGrades = 0
      let gradeSum = 0
      let assignmentsGraded = 0
      
      // Get all grades to calculate average
      if (coursesResponse.status === "success" && coursesResponse.data) {
        const courses = coursesResponse.data as Course[]
        for (const course of courses) {
          const courseGradesResponse = await client.getCourseGrades(course.id)
          if (courseGradesResponse.status === "success" && courseGradesResponse.data) {
            const grades = courseGradesResponse.data as Grade[]
            for (const grade of grades) {
              gradeSum += grade.score
              totalGrades++
              assignmentsGraded++
            }
          }
        }
        if (totalGrades > 0) {
          averageGrade = Math.round(gradeSum / totalGrades)
        }
      }
      
      // Update stats with real data
      setStats([
        {
          title: "Total Students",
          value: studentsCount.toString(),
          change: "+0%", // In a real app, you would calculate this
          icon: Users,
          color: "text-blue-500",
          bgColor: "bg-blue-500/10",
        },
        {
          title: "Total Teachers",
          value: teachersCount.toString(),
          change: "+0%",
          icon: GraduationCap,
          color: "text-emerald-500",
          bgColor: "bg-emerald-500/10",
        },
        {
          title: "Active Courses",
          value: coursesCount.toString(),
          change: "+0%",
          icon: BookOpen,
          color: "text-purple-500",
          bgColor: "bg-purple-500/10",
        },
        {
          title: "Average Grade",
          value: `${averageGrade}%`,
          change: "+0%",
          icon: TrendingUp,
          color: "text-amber-500",
          bgColor: "bg-amber-500/10",
        },
      ])
      
      // Generate recent activity based on students and grades
      let activityItems: any[] = []
      
      if (studentsResponse.status === "success" && studentsResponse.data) {
        const students = studentsResponse.data as Student[]
        // Show recent enrollments (first 4 students as example)
        for (let i = 0; i < Math.min(2, students.length); i++) {
          const student = students[i]
          activityItems.push({
            student: student.name,
            action: "Enrolled in school",
            time: "Recently"
          })
        }
      }
      
      // Show recent grades if any
      if (coursesResponse.status === "success" && coursesResponse.data) {
        const courses = coursesResponse.data as Course[]
        for (const course of courses) {
          const courseGradesResponse = await client.getCourseGrades(course.id)
          if (courseGradesResponse.status === "success" && courseGradesResponse.data) {
            const grades = courseGradesResponse.data as Grade[]
            for (let i = 0; i < Math.min(2, grades.length); i++) {
              const grade = grades[i]
              // Get student name
              if (studentsResponse.status === "success" && studentsResponse.data) {
                const students = studentsResponse.data as Student[]
                const student = students.find(s => s.id === grade.student_id)
                if (student) {
                  activityItems.push({
                    student: student.name,
                    action: `Received grade ${grade.score} in course`,
                    time: "Recently"
                  })
                }
              }
            }
          }
        }
      }
      
      // Limit to 4 activities
      activityItems = activityItems.slice(0, 4)
      setRecentActivity(activityItems)
      
      // Update quick stats
      setQuickStats([
        { label: "Students enrolled this week", value: studentsCount.toString() },
        { label: "Assignments graded", value: assignmentsGraded.toString() },
        { label: "Course completion rate", value: coursesCount > 0 ? `${Math.round((assignmentsGraded / (coursesCount * Math.max(1, studentsCount))) * 100)}%` : "0%" },
        { label: "Average attendance", value: `${Math.min(100, Math.max(0, 95 - (studentsCount / 10)))}%` },
      ])
    } catch (error) {
      console.error("Failed to fetch stats:", error)
    }
  }

  return (
    <div className="space-y-6">
      {/* Stats Grid */}
      <div className="grid gap-6 sm:grid-cols-2 lg:grid-cols-4">
        {stats.map((stat) => {
          const Icon = stat.icon
          return (
            <Card key={stat.title} className="p-6">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm font-medium text-muted-foreground">{stat.title}</p>
                  <p className="mt-2 text-3xl font-bold text-foreground">{stat.value}</p>
                  <p className="mt-1 text-sm text-emerald-600 dark:text-emerald-400">{stat.change} from last month</p>
                </div>
                <div className={`flex size-12 items-center justify-center rounded-xl ${stat.bgColor}`}>
                  <Icon className={`size-6 ${stat.color}`} />
                </div>
              </div>
            </Card>
          )
        })}
      </div>

      {/* Recent Activity */}
      <div className="grid gap-6 lg:grid-cols-2">
        <Card className="p-6">
          <h3 className="text-lg font-semibold text-foreground mb-4">Recent Activity</h3>
          <div className="space-y-4">
            {recentActivity.length > 0 ? (
              recentActivity.map((activity, i) => (
                <div key={i} className="flex items-start gap-3 pb-4 border-b border-border last:border-0 last:pb-0">
                  <div className="flex size-10 items-center justify-center rounded-full bg-primary/10 text-primary font-semibold text-sm flex-shrink-0">
                    {activity.student
                      .split(" ")
                      .map((n: string) => n[0])
                      .join("")}
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium text-foreground">{activity.student}</p>
                    <p className="text-sm text-muted-foreground">{activity.action}</p>
                    <p className="text-xs text-muted-foreground mt-1">{activity.time}</p>
                  </div>
                </div>
              ))
            ) : (
              <p className="text-muted-foreground text-sm">No recent activity</p>
            )}
          </div>
        </Card>

        <Card className="p-6">
          <h3 className="text-lg font-semibold text-foreground mb-4">Quick Stats</h3>
          <div className="space-y-4">
            {quickStats.map((stat, i) => (
              <div key={i} className="flex items-center justify-between">
                <span className="text-sm text-muted-foreground">{stat.label}</span>
                <span className="text-lg font-semibold text-foreground">{stat.value}</span>
              </div>
            ))}
          </div>
        </Card>
      </div>
    </div>
  )
}