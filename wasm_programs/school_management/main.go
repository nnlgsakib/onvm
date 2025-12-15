package main

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"strings"
	"unsafe"
)

// Student represents a student entity
type Student struct {
	ID       string   `json:"id"`
	Name     string   `json:"name"`
	Grade    string   `json:"grade"`
	Age      int      `json:"age"`
	Email    string   `json:"email"`
	Subjects []string `json:"subjects"`
}

// Teacher represents a teacher entity
type Teacher struct {
	ID      string `json:"id"`
	Name    string `json:"name"`
	Subject string `json:"subject"`
	Email   string `json:"email"`
	Phone   string `json:"phone"`
}

// Course represents a course entity
type Course struct {
	ID          string   `json:"id"`
	Name        string   `json:"name"`
	Code        string   `json:"code"`
	TeacherID   string   `json:"teacher_id"`
	StudentIDs  []string `json:"student_ids"`
	Description string   `json:"description"`
}

// Grade represents a grade record
type Grade struct {
	ID        string `json:"id"`
	StudentID string `json:"student_id"`
	CourseID  string `json:"course_id"`
	Score     int    `json:"score"`
	Date      string `json:"date"`
}

// Request represents the incoming request
type Request struct {
	Operation string      `json:"operation"`
	Data      interface{} `json:"data,omitempty"`
}

// Response represents the outgoing response
type Response struct {
	Status  string      `json:"status"`
	Message string      `json:"message,omitempty"`
	Data    interface{} `json:"data,omitempty"`
	Error   string      `json:"error,omitempty"`
}

// Entity types for our operations
type AddStudentRequest struct {
	Student Student `json:"student"`
}

type AddTeacherRequest struct {
	Teacher Teacher `json:"teacher"`
}

type AddCourseRequest struct {
	Course Course `json:"course"`
}

type EnrollStudentRequest struct {
	StudentID string `json:"student_id"`
	CourseID  string `json:"course_id"`
}

type AssignGradeRequest struct {
	Grade Grade `json:"grade"`
}

type GetStudentsByGradeRequest struct {
	Grade string `json:"grade"`
}

// Global variables for state management
var (
	outBuf      []byte
	keyIndexKey = "__school_keys"
)

func main() {
	// This is required for TinyGo to compile properly
}

//export onvm_main
func onvm_main(ptr int32, length int32) (int32, int32) {
	input := ptrToString(ptr, length)
	response := handleRequest(input)

	// Encode response to JSON
	respJSON, err := json.Marshal(response)
	if err != nil {
		errorResp := Response{
			Status: "error",
			Error:  "Failed to encode response: " + err.Error(),
		}
		respJSON, _ = json.Marshal(errorResp)
	}

	// Get the length of the response
	respLen := len(respJSON)

	// Prepare output buffer
	outBuf = make([]byte, respLen)
	copy(outBuf, respJSON)

	return int32(uintptr(unsafe.Pointer(&outBuf[0]))), int32(respLen)
}

//export onvm_last_result
func onvm_last_result() (int32, int32) {
	bufLen := len(outBuf)
	if bufLen == 0 {
		return 0, 0
	}
	return int32(uintptr(unsafe.Pointer(&outBuf[0]))), int32(bufLen)
}

func handleRequest(input string) Response {
	var req Request
	if err := json.Unmarshal([]byte(input), &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid JSON: " + err.Error(),
		}
	}

	switch req.Operation {
	case "add_student":
		return handleAddStudent(req.Data)
	case "get_student":
		return handleGetStudent(req.Data)
	case "update_student":
		return handleUpdateStudent(req.Data)
	case "delete_student":
		return handleDeleteStudent(req.Data)
	case "list_students":
		return handleListStudents(req.Data)
	case "add_teacher":
		return handleAddTeacher(req.Data)
	case "get_teacher":
		return handleGetTeacher(req.Data)
	case "update_teacher":
		return handleUpdateTeacher(req.Data)
	case "delete_teacher":
		return handleDeleteTeacher(req.Data)
	case "list_teachers":
		return handleListTeachers(req.Data)
	case "add_course":
		return handleAddCourse(req.Data)
	case "get_course":
		return handleGetCourse(req.Data)
	case "update_course":
		return handleUpdateCourse(req.Data)
	case "delete_course":
		return handleDeleteCourse(req.Data)
	case "list_courses":
		return handleListCourses(req.Data)
	case "enroll_student":
		return handleEnrollStudent(req.Data)
	case "assign_grade":
		return handleAssignGrade(req.Data)
	case "get_student_grades":
		return handleGetStudentGrades(req.Data)
	case "get_course_grades":
		return handleGetCourseGrades(req.Data)
	case "get_students_by_grade":
		return handleGetStudentsByGrade(req.Data)
	default:
		return Response{
			Status: "error",
			Error:  "Unknown operation: " + req.Operation,
		}
	}
}

// Student Operations
func handleAddStudent(data interface{}) Response {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal data: " + err.Error(),
		}
	}

	var req AddStudentRequest
	if err := json.Unmarshal(jsonData, &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid add student request: " + err.Error(),
		}
	}

	// Store student
	studentKey := "student_" + req.Student.ID
	studentData, err := json.Marshal(req.Student)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal student: " + err.Error(),
		}
	}

	err = statePut([]byte(studentKey), studentData)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to store student: " + err.Error(),
		}
	}

	// Update index
	err = updateKeyIndex(studentKey)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to update index: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Student added successfully",
		Data:    req.Student,
	}
}

func handleGetStudent(data interface{}) Response {
	studentID, ok := data.(string)
	if !ok {
		return Response{
			Status: "error",
			Error:  "Invalid student ID",
		}
	}

	studentKey := "student_" + studentID
	studentData, err := stateGet([]byte(studentKey))
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to retrieve student: " + err.Error(),
		}
	}

	if len(studentData) == 0 {
		return Response{
			Status: "error",
			Error:  "Student not found",
		}
	}

	var student Student
	if err := json.Unmarshal(studentData, &student); err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to unmarshal student: " + err.Error(),
		}
	}

	return Response{
		Status: "success",
		Data:   student,
	}
}

func handleUpdateStudent(data interface{}) Response {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal data: " + err.Error(),
		}
	}

	var req AddStudentRequest
	if err := json.Unmarshal(jsonData, &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid update student request: " + err.Error(),
		}
	}

	// Update student
	studentKey := "student_" + req.Student.ID
	studentData, err := json.Marshal(req.Student)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal student: " + err.Error(),
		}
	}

	err = statePut([]byte(studentKey), studentData)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to update student: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Student updated successfully",
		Data:    req.Student,
	}
}

func handleDeleteStudent(data interface{}) Response {
	studentID, ok := data.(string)
	if !ok {
		return Response{
			Status: "error",
			Error:  "Invalid student ID",
		}
	}

	studentKey := "student_" + studentID
	err := statePut([]byte(studentKey), []byte{})
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to delete student: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Student deleted successfully",
	}
}

func handleListStudents(data interface{}) Response {
	keys, err := loadKeyIndex()
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to load key index: " + err.Error(),
		}
	}

	var students []Student
	for _, key := range keys {
		if strings.HasPrefix(key, "student_") {
			studentData, err := stateGet([]byte(key))
			if err != nil {
				continue // Skip invalid entries
			}
			if len(studentData) > 0 {
				var student Student
				if err := json.Unmarshal(studentData, &student); err == nil {
					students = append(students, student)
				}
			}
		}
	}

	return Response{
		Status: "success",
		Data:   students,
	}
}

// Teacher Operations
func handleAddTeacher(data interface{}) Response {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal data: " + err.Error(),
		}
	}

	var req AddTeacherRequest
	if err := json.Unmarshal(jsonData, &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid add teacher request: " + err.Error(),
		}
	}

	// Store teacher
	teacherKey := "teacher_" + req.Teacher.ID
	teacherData, err := json.Marshal(req.Teacher)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal teacher: " + err.Error(),
		}
	}

	err = statePut([]byte(teacherKey), teacherData)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to store teacher: " + err.Error(),
		}
	}

	// Update index
	err = updateKeyIndex(teacherKey)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to update index: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Teacher added successfully",
		Data:    req.Teacher,
	}
}

func handleGetTeacher(data interface{}) Response {
	teacherID, ok := data.(string)
	if !ok {
		return Response{
			Status: "error",
			Error:  "Invalid teacher ID",
		}
	}

	teacherKey := "teacher_" + teacherID
	teacherData, err := stateGet([]byte(teacherKey))
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to retrieve teacher: " + err.Error(),
		}
	}

	if len(teacherData) == 0 {
		return Response{
			Status: "error",
			Error:  "Teacher not found",
		}
	}

	var teacher Teacher
	if err := json.Unmarshal(teacherData, &teacher); err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to unmarshal teacher: " + err.Error(),
		}
	}

	return Response{
		Status: "success",
		Data:   teacher,
	}
}

func handleUpdateTeacher(data interface{}) Response {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal data: " + err.Error(),
		}
	}

	var req AddTeacherRequest
	if err := json.Unmarshal(jsonData, &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid update teacher request: " + err.Error(),
		}
	}

	// Update teacher
	teacherKey := "teacher_" + req.Teacher.ID
	teacherData, err := json.Marshal(req.Teacher)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal teacher: " + err.Error(),
		}
	}

	err = statePut([]byte(teacherKey), teacherData)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to update teacher: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Teacher updated successfully",
		Data:    req.Teacher,
	}
}

func handleDeleteTeacher(data interface{}) Response {
	teacherID, ok := data.(string)
	if !ok {
		return Response{
			Status: "error",
			Error:  "Invalid teacher ID",
		}
	}

	teacherKey := "teacher_" + teacherID
	err := statePut([]byte(teacherKey), []byte{})
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to delete teacher: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Teacher deleted successfully",
	}
}

func handleListTeachers(data interface{}) Response {
	keys, err := loadKeyIndex()
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to load key index: " + err.Error(),
		}
	}

	var teachers []Teacher
	for _, key := range keys {
		if strings.HasPrefix(key, "teacher_") {
			teacherData, err := stateGet([]byte(key))
			if err != nil {
				continue // Skip invalid entries
			}
			if len(teacherData) > 0 {
				var teacher Teacher
				if err := json.Unmarshal(teacherData, &teacher); err == nil {
					teachers = append(teachers, teacher)
				}
			}
		}
	}

	return Response{
		Status: "success",
		Data:   teachers,
	}
}

// Course Operations
func handleAddCourse(data interface{}) Response {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal data: " + err.Error(),
		}
	}

	var req AddCourseRequest
	if err := json.Unmarshal(jsonData, &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid add course request: " + err.Error(),
		}
	}

	// Store course
	courseKey := "course_" + req.Course.ID
	courseData, err := json.Marshal(req.Course)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal course: " + err.Error(),
		}
	}

	err = statePut([]byte(courseKey), courseData)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to store course: " + err.Error(),
		}
	}

	// Update index
	err = updateKeyIndex(courseKey)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to update index: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Course added successfully",
		Data:    req.Course,
	}
}

func handleGetCourse(data interface{}) Response {
	courseID, ok := data.(string)
	if !ok {
		return Response{
			Status: "error",
			Error:  "Invalid course ID",
		}
	}

	courseKey := "course_" + courseID
	courseData, err := stateGet([]byte(courseKey))
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to retrieve course: " + err.Error(),
		}
	}

	if len(courseData) == 0 {
		return Response{
			Status: "error",
			Error:  "Course not found",
		}
	}

	var course Course
	if err := json.Unmarshal(courseData, &course); err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to unmarshal course: " + err.Error(),
		}
	}

	return Response{
		Status: "success",
		Data:   course,
	}
}

func handleUpdateCourse(data interface{}) Response {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal data: " + err.Error(),
		}
	}

	var req AddCourseRequest
	if err := json.Unmarshal(jsonData, &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid update course request: " + err.Error(),
		}
	}

	// Update course
	courseKey := "course_" + req.Course.ID
	courseData, err := json.Marshal(req.Course)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal course: " + err.Error(),
		}
	}

	err = statePut([]byte(courseKey), courseData)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to update course: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Course updated successfully",
		Data:    req.Course,
	}
}

func handleDeleteCourse(data interface{}) Response {
	courseID, ok := data.(string)
	if !ok {
		return Response{
			Status: "error",
			Error:  "Invalid course ID",
		}
	}

	courseKey := "course_" + courseID
	err := statePut([]byte(courseKey), []byte{})
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to delete course: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Course deleted successfully",
	}
}

func handleListCourses(data interface{}) Response {
	keys, err := loadKeyIndex()
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to load key index: " + err.Error(),
		}
	}

	var courses []Course
	for _, key := range keys {
		if strings.HasPrefix(key, "course_") {
			courseData, err := stateGet([]byte(key))
			if err != nil {
				continue // Skip invalid entries
			}
			if len(courseData) > 0 {
				var course Course
				if err := json.Unmarshal(courseData, &course); err == nil {
					courses = append(courses, course)
				}
			}
		}
	}

	return Response{
		Status: "success",
		Data:   courses,
	}
}

// Enrollment Operations
func handleEnrollStudent(data interface{}) Response {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal data: " + err.Error(),
		}
	}

	var req EnrollStudentRequest
	if err := json.Unmarshal(jsonData, &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid enroll student request: " + err.Error(),
		}
	}

	// Get course
	courseKey := "course_" + req.CourseID
	courseData, err := stateGet([]byte(courseKey))
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to retrieve course: " + err.Error(),
		}
	}

	if len(courseData) == 0 {
		return Response{
			Status: "error",
			Error:  "Course not found",
		}
	}

	var course Course
	if err := json.Unmarshal(courseData, &course); err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to unmarshal course: " + err.Error(),
		}
	}

	// Check if student is already enrolled
	for _, id := range course.StudentIDs {
		if id == req.StudentID {
			return Response{
				Status:  "success",
				Message: "Student already enrolled in course",
			}
		}
	}

	// Add student to course
	course.StudentIDs = append(course.StudentIDs, req.StudentID)
	updatedCourseData, err := json.Marshal(course)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal updated course: " + err.Error(),
		}
	}

	err = statePut([]byte(courseKey), updatedCourseData)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to update course: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Student enrolled successfully",
		Data:    course,
	}
}

// Grade Operations
func handleAssignGrade(data interface{}) Response {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal data: " + err.Error(),
		}
	}

	var req AssignGradeRequest
	if err := json.Unmarshal(jsonData, &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid assign grade request: " + err.Error(),
		}
	}

	// Store grade
	gradeKey := "grade_" + req.Grade.ID
	gradeData, err := json.Marshal(req.Grade)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal grade: " + err.Error(),
		}
	}

	err = statePut([]byte(gradeKey), gradeData)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to store grade: " + err.Error(),
		}
	}

	// Update index
	err = updateKeyIndex(gradeKey)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to update index: " + err.Error(),
		}
	}

	return Response{
		Status:  "success",
		Message: "Grade assigned successfully",
		Data:    req.Grade,
	}
}

func handleGetStudentGrades(data interface{}) Response {
	studentID, ok := data.(string)
	if !ok {
		return Response{
			Status: "error",
			Error:  "Invalid student ID",
		}
	}

	keys, err := loadKeyIndex()
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to load key index: " + err.Error(),
		}
	}

	var grades []Grade
	for _, key := range keys {
		if strings.HasPrefix(key, "grade_") {
			gradeData, err := stateGet([]byte(key))
			if err != nil {
				continue // Skip invalid entries
			}
			if len(gradeData) > 0 {
				var grade Grade
				if err := json.Unmarshal(gradeData, &grade); err == nil {
					if grade.StudentID == studentID {
						grades = append(grades, grade)
					}
				}
			}
		}
	}

	return Response{
		Status: "success",
		Data:   grades,
	}
}

func handleGetCourseGrades(data interface{}) Response {
	courseID, ok := data.(string)
	if !ok {
		return Response{
			Status: "error",
			Error:  "Invalid course ID",
		}
	}

	keys, err := loadKeyIndex()
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to load key index: " + err.Error(),
		}
	}

	var grades []Grade
	for _, key := range keys {
		if strings.HasPrefix(key, "grade_") {
			gradeData, err := stateGet([]byte(key))
			if err != nil {
				continue // Skip invalid entries
			}
			if len(gradeData) > 0 {
				var grade Grade
				if err := json.Unmarshal(gradeData, &grade); err == nil {
					if grade.CourseID == courseID {
						grades = append(grades, grade)
					}
				}
			}
		}
	}

	return Response{
		Status: "success",
		Data:   grades,
	}
}

func handleGetStudentsByGrade(data interface{}) Response {
	jsonData, err := json.Marshal(data)
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to marshal data: " + err.Error(),
		}
	}

	var req GetStudentsByGradeRequest
	if err := json.Unmarshal(jsonData, &req); err != nil {
		return Response{
			Status: "error",
			Error:  "Invalid get students by grade request: " + err.Error(),
		}
	}

	keys, err := loadKeyIndex()
	if err != nil {
		return Response{
			Status: "error",
			Error:  "Failed to load key index: " + err.Error(),
		}
	}

	var students []Student
	for _, key := range keys {
		if strings.HasPrefix(key, "student_") {
			studentData, err := stateGet([]byte(key))
			if err != nil {
				continue // Skip invalid entries
			}
			if len(studentData) > 0 {
				var student Student
				if err := json.Unmarshal(studentData, &student); err == nil {
					if student.Grade == req.Grade {
						students = append(students, student)
					}
				}
			}
		}
	}

	return Response{
		Status: "success",
		Data:   students,
	}
}

// Helper functions for state management
func statePut(key, value []byte) error {
	rc := onvm_state_put(
		int32(uintptr(unsafe.Pointer(&key[0]))),
		int32(len(key)),
		int32(uintptr(unsafe.Pointer(&value[0]))),
		int32(len(value)),
	)
	if rc != 0 {
		return fmt.Errorf("state put failed with code: %d", rc)
	}
	return nil
}

func stateGet(key []byte) ([]byte, error) {
	// First, try to get the size needed
	size := onvm_state_get(
		int32(uintptr(unsafe.Pointer(&key[0]))),
		int32(len(key)),
		0,
		0,
	)
	
	// If size is 0, key doesn't exist
	if size == 0 {
		return []byte{}, nil
	}
	
	// If size is negative, it indicates the needed buffer size (but as a negative value)
	if size < 0 {
		size = -size
	}
	
	// Allocate buffer and get the actual data
	buf := make([]byte, size)
	actualSize := onvm_state_get(
		int32(uintptr(unsafe.Pointer(&key[0]))),
		int32(len(key)),
		int32(uintptr(unsafe.Pointer(&buf[0]))),
		int32(size),
	)
	
	// Check the result
	if actualSize == 0 {
		// Key not found
		return []byte{}, nil
	} else if actualSize < 0 {
		// Error occurred
		return nil, fmt.Errorf("state get failed with code: %d", actualSize)
	}
	
	// Success - return the data
	return buf[:actualSize], nil
}

func updateKeyIndex(newKey string) error {
	// Get current index
	indexData, err := stateGet([]byte(keyIndexKey))
	if err != nil {
		return err
	}

	var keys []string
	if len(indexData) > 0 {
		decoded, err := base64.StdEncoding.DecodeString(string(indexData))
		if err != nil {
			return err
		}
		if len(decoded) > 0 {
			keysStr := string(decoded)
			if keysStr != "" {
				keys = strings.Split(keysStr, ",")
			}
		}
	}

	// Check if key already exists
	exists := false
	for _, key := range keys {
		if key == newKey {
			exists = true
			break
		}
	}

	// Add key if it doesn't exist
	if !exists {
		keys = append(keys, newKey)
		joined := strings.Join(keys, ",")
		encoded := base64.StdEncoding.EncodeToString([]byte(joined))
		return statePut([]byte(keyIndexKey), []byte(encoded))
	}

	return nil
}

func loadKeyIndex() ([]string, error) {
	indexData, err := stateGet([]byte(keyIndexKey))
	if err != nil {
		return nil, err
	}

	if len(indexData) == 0 {
		return []string{}, nil
	}

	decoded, err := base64.StdEncoding.DecodeString(string(indexData))
	if err != nil {
		return nil, err
	}

	if len(decoded) == 0 {
		return []string{}, nil
	}

	keysStr := string(decoded)
	if keysStr == "" {
		return []string{}, nil
	}

	return strings.Split(keysStr, ","), nil
}

// Utility function to convert pointer and length to string
func ptrToString(ptr int32, length int32) string {
	return string((*[1 << 30]byte)(unsafe.Pointer(uintptr(ptr)))[:length:length])
}

// Host function declarations
//
//go:wasm-module env
//export onvm_state_put
func onvm_state_put(key_ptr int32, key_len int32, val_ptr int32, val_len int32) int32

//
//go:wasm-module env
//export onvm_state_get
func onvm_state_get(key_ptr int32, key_len int32, out_ptr int32, out_cap int32) int32

//
//go:wasm-module env
//export onvm_state_root
func onvm_state_root(out_ptr int32) int32