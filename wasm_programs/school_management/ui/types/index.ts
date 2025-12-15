export interface Student {
  id: string;
  name: string;
  grade: string;
  age: number;
  email: string;
  subjects: string[];
}

export interface Teacher {
  id: string;
  name: string;
  subject: string;
  email: string;
  phone: string;
}

export interface Course {
  id: string;
  name: string;
  code: string;
  teacher_id: string;
  student_ids: string[];
  description: string;
}

export interface Grade {
  id: string;
  student_id: string;
  course_id: string;
  score: number;
  date: string;
}