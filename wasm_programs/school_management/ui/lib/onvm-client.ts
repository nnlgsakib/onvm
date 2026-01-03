import { OnvmClient } from 'onvm-sdk';
import type {
  Student,
  Teacher,
  Course,
  Grade
} from '@/types'

const RPC_ENDPOINT = process.env.NEXT_PUBLIC_RPC_URL || 'http://localhost:8081';
const PROGRAM_ID = process.env.NEXT_PUBLIC_SCHOOL_PROGRAM_ID || "prog618df68d440c4f7ac9dae7f63a36ff8e10fb3bb970c7e7aa3d00b4195e06bbd7";
const PROJECT_ID = process.env.NEXT_PUBLIC_PROJECT_ID;
const PROJECT_SECRET = process.env.NEXT_PUBLIC_PROJECT_SECRET;

export class SchoolManagementClient {
  private client: OnvmClient;
  private programId: string;

  constructor() {
    this.client = new OnvmClient({
      rpcUrl: RPC_ENDPOINT,
      programId: PROGRAM_ID,
      projectId: PROJECT_ID,
      projectSecret: PROJECT_SECRET,
      timeout: 30000,
    });

    this.programId = PROGRAM_ID;
  }

  async addStudent(student: Student): Promise<any> {
    return this.executeProgram({
      operation: 'add_student',
      data: { student }
    });
  }

  async getStudent(id: string): Promise<any> {
    return this.executeProgram({
      operation: 'get_student',
      data: id
    });
  }

  async updateStudent(student: Student): Promise<any> {
    return this.executeProgram({
      operation: 'update_student',
      data: { student }
    });
  }

  async deleteStudent(id: string): Promise<any> {
    return this.executeProgram({
      operation: 'delete_student',
      data: id
    });
  }

  async listStudents(): Promise<any> {
    return this.executeProgram({
      operation: 'list_students',
      data: null
    });
  }

  async addTeacher(teacher: Teacher): Promise<any> {
    return this.executeProgram({
      operation: 'add_teacher',
      data: { teacher }
    });
  }

  async getTeacher(id: string): Promise<any> {
    return this.executeProgram({
      operation: 'get_teacher',
      data: id
    });
  }

  async updateTeacher(teacher: Teacher): Promise<any> {
    return this.executeProgram({
      operation: 'update_teacher',
      data: { teacher }
    });
  }

  async deleteTeacher(id: string): Promise<any> {
    return this.executeProgram({
      operation: 'delete_teacher',
      data: id
    });
  }

  async listTeachers(): Promise<any> {
    return this.executeProgram({
      operation: 'list_teachers',
      data: null
    });
  }

  async addCourse(course: Course): Promise<any> {
    return this.executeProgram({
      operation: 'add_course',
      data: { course }
    });
  }

  async getCourse(id: string): Promise<any> {
    return this.executeProgram({
      operation: 'get_course',
      data: id
    });
  }

  async updateCourse(course: Course): Promise<any> {
    return this.executeProgram({
      operation: 'update_course',
      data: { course }
    });
  }

  async deleteCourse(id: string): Promise<any> {
    return this.executeProgram({
      operation: 'delete_course',
      data: id
    });
  }

  async listCourses(): Promise<any> {
    return this.executeProgram({
      operation: 'list_courses',
      data: null
    });
  }

  async assignGrade(grade: Grade): Promise<any> {
    return this.executeProgram({
      operation: 'assign_grade',
      data: { grade }
    });
  }

  async getStudentGrades(studentId: string): Promise<any> {
    return this.executeProgram({
      operation: 'get_student_grades',
      data: studentId
    });
  }

  async getCourseGrades(courseId: string): Promise<any> {
    return this.executeProgram({
      operation: 'get_course_grades',
      data: courseId
    });
  }

  async enrollStudent(studentId: string, courseId: string): Promise<any> {
    return this.executeProgram({
      operation: 'enroll_student',
      data: { student_id: studentId, course_id: courseId }
    });
  }

  private async executeProgram<T extends { status: string }>(
    request: any
  ): Promise<T> {
    const inputBase64 = Buffer.from(JSON.stringify(request)).toString('base64');

    const result = await this.client.executeProgram({
      program_id: this.programId,
      input_base64: inputBase64,
    });

    const returnData = Buffer.from(result.return_base64, 'base64').toString('utf-8');
    const parsedResponse = JSON.parse(returnData);

    if (parsedResponse.status === 'error') {
      throw new Error(parsedResponse.error || 'Unknown error occurred');
    }

    return parsedResponse as T;
  }
}

let schoolClient: SchoolManagementClient | null = null;

export const getSchoolClient = (): SchoolManagementClient => {
  if (!schoolClient) {
    schoolClient = new SchoolManagementClient();
  }
  return schoolClient;
};

export default SchoolManagementClient;
