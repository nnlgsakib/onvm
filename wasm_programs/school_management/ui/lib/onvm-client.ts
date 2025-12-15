// ONVM Client for School Management WASM Program
// This client interacts with the ONVM RPC to manage school entities

import type {
  Student,
  Teacher,
  Course,
  Grade
} from '@/types'

const RPC_ENDPOINT = 'http://localhost:8081';

export class SchoolManagementClient {
  private rpcEndpoint: string;
  private programId: string;

  constructor(rpcEndpoint: string = RPC_ENDPOINT, programId: string = "81b81270820fb1580b91f4b0e3bc249cfa5126a832ea4d46e6bf921cf1a6f0c8") {
    this.rpcEndpoint = rpcEndpoint;
    this.programId = programId;
  }

  // Student operations
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

  // Teacher operations
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

  // Course operations
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

  // Grade operations
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

  // Enrollment operations
  async enrollStudent(studentId: string, courseId: string): Promise<any> {
    return this.executeProgram({
      operation: 'enroll_student',
      data: { student_id: studentId, course_id: courseId }
    });
  }

  // Generic execute program method
  private async executeProgram<T extends { status: string }>(
    request: any
  ): Promise<T> {
    try {
      if (!this.programId) {
        throw new Error('PROGRAM_ID is not set');
      }

      // Encode request as base64
      const inputBase64 = Buffer.from(JSON.stringify(request)).toString('base64');

      // Execute program via ONVM RPC
      const response = await fetch(`${this.rpcEndpoint}/execute`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          program_id: this.programId,
          input_base64: inputBase64,
        }),
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`RPC error: ${response.status} - ${errorText}`);
      }

      const result = await response.json();
      
      // Decode the response
      const returnData = Buffer.from(result.return_base64, 'base64').toString('utf-8');
      const parsedResponse = JSON.parse(returnData);
      
      if (parsedResponse.status === 'error') {
        throw new Error(parsedResponse.error || 'Unknown error occurred');
      }
      
      return parsedResponse as T;
    } catch (error) {
      console.error('ONVM Client Error:', error);
      throw error;
    }
  }

  // Helper method to upload a WASM program
  async uploadWasm(wasmBuffer: ArrayBuffer): Promise<string> {
    try {
      // Upload WASM blob
      const blobResponse = await fetch(`${this.rpcEndpoint}/blobs`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/wasm',
        },
        body: wasmBuffer,
      });

      if (!blobResponse.ok) {
        const errorText = await blobResponse.text();
        throw new Error(`Blob upload error: ${blobResponse.status} - ${errorText}`);
      }

      const blobResult = await blobResponse.json();
      const blobId = blobResult.id;

      // Deploy program
      const deployResponse = await fetch(`${this.rpcEndpoint}/programs`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          wasm_base64: Buffer.from(wasmBuffer).toString('base64'),
          entrypoint: 'onvm_main',
          blob_refs: [],
        }),
      });

      if (!deployResponse.ok) {
        const errorText = await deployResponse.text();
        throw new Error(`Program deploy error: ${deployResponse.status} - ${errorText}`);
      }

      const deployResult = await deployResponse.json();
      return deployResult.id;
    } catch (error) {
      // Wasm Upload Error:', error);
      throw error;
    }
  }
}

let schoolClient: SchoolManagementClient | null = null;

export const getSchoolClient = (): SchoolManagementClient => {
  if (!schoolClient) {
    schoolClient = new SchoolManagementClient(RPC_ENDPOINT, "ddce10e9bc27953dac0458ac16aa9908aea871b97ec76602c06dc5efaac3fd40");
  }
  return schoolClient;
};


export default SchoolManagementClient;