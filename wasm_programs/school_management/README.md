# School Management System WASM Program

This is a WebAssembly program written in Go (using TinyGo) for the ONVM platform that implements a comprehensive school management system.

## Features

- Student management (add, update, delete, list)
- Teacher management (add, update, delete, list)
- Course management (add, update, delete, list)
- Student enrollment in courses
- Grade assignment and tracking
- Reporting capabilities

## Prerequisites

To build and deploy this program, you need:

1. [TinyGo](https://tinygo.org/getting-started/) (for building WASM)
2. [ONVM CLI](https://github.com/onvm/onvm) (for deployment)

## Building

### On Linux/macOS:
```bash
./build.sh
```

### On Windows:
```cmd
build.bat
```

Or manually:
```bash
tinygo build -o school_management.wasm -target wasi main.go
```

## Deployment

### On Linux/macOS:
```bash
./deploy.sh
```

### On Windows:
```cmd
deploy.bat
```

Or manually:
```bash
onvm deploy --file school_management.wasm --entrypoint onvm_main
```

## API Operations

The program supports the following operations:

### Student Operations
- `add_student` - Add a new student
- `get_student` - Retrieve a student by ID
- `update_student` - Update student information
- `delete_student` - Delete a student
- `list_students` - List all students
- `get_students_by_grade` - Get students by grade level

### Teacher Operations
- `add_teacher` - Add a new teacher
- `get_teacher` - Retrieve a teacher by ID
- `update_teacher` - Update teacher information
- `delete_teacher` - Delete a teacher
- `list_teachers` - List all teachers

### Course Operations
- `add_course` - Add a new course
- `get_course` - Retrieve a course by ID
- `update_course` - Update course information
- `delete_course` - Delete a course
- `list_courses` - List all courses

### Enrollment Operations
- `enroll_student` - Enroll a student in a course

### Grade Operations
- `assign_grade` - Assign a grade to a student for a course
- `get_student_grades` - Get all grades for a student
- `get_course_grades` - Get all grades for a course

## Example Usage

### Add a Student
```json
{
  "operation": "add_student",
  "data": {
    "student": {
      "id": "s001",
      "name": "John Doe",
      "grade": "10th",
      "age": 16,
      "email": "john.doe@example.com",
      "subjects": ["Math", "Science", "English"]
    }
  }
}
```

### Add a Teacher
```json
{
  "operation": "add_teacher",
  "data": {
    "teacher": {
      "id": "t001",
      "name": "Jane Smith",
      "subject": "Mathematics",
      "email": "jane.smith@example.com",
      "phone": "+1-555-0123"
    }
  }
}
```

### Add a Course
```json
{
  "operation": "add_course",
  "data": {
    "course": {
      "id": "c001",
      "name": "Algebra I",
      "code": "MATH-101",
      "teacher_id": "t001",
      "student_ids": [],
      "description": "Introduction to algebraic concepts"
    }
  }
}
```

### Enroll Student in Course
```json
{
  "operation": "enroll_student",
  "data": {
    "student_id": "s001",
    "course_id": "c001"
  }
}
```

### Assign Grade
```json
{
  "operation": "assign_grade",
  "data": {
    "grade": {
      "id": "g001",
      "student_id": "s001",
      "course_id": "c001",
      "score": 95,
      "date": "2023-10-15"
    }
  }
}
```

### Get Student Grades
```json
{
  "operation": "get_student_grades",
  "data": "s001"
}
```

### List All Students
```json
{
  "operation": "list_students",
  "data": null
}
```

## Response Format

All responses follow this format:
```json
{
  "status": "success|error",
  "message": "Optional message",
  "data": "Response data (varies by operation)",
  "error": "Error message (only present on error)"
}
```

## Data Models

### Student
```json
{
  "id": "Unique identifier",
  "name": "Full name",
  "grade": "Grade level",
  "age": "Age in years",
  "email": "Email address",
  "subjects": ["Array of subjects"]
}
```

### Teacher
```json
{
  "id": "Unique identifier",
  "name": "Full name",
  "subject": "Primary subject taught",
  "email": "Email address",
  "phone": "Phone number"
}
```

### Course
```json
{
  "id": "Unique identifier",
  "name": "Course name",
  "code": "Course code",
  "teacher_id": "ID of assigned teacher",
  "student_ids": ["Array of enrolled student IDs"],
  "description": "Course description"
}
```

### Grade
```json
{
  "id": "Unique identifier",
  "student_id": "ID of student",
  "course_id": "ID of course",
  "score": "Numeric score",
  "date": "Date in YYYY-MM-DD format"
}
```

## Error Handling

All errors are returned in the following format:
```json
{
  "status": "error",
  "error": "Description of the error"
}
```

Common error types:
- Invalid JSON input
- Missing required fields
- Entity not found
- Storage errors