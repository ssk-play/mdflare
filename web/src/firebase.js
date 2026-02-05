import { initializeApp } from 'firebase/app';
import { getDatabase, ref, set, onValue } from 'firebase/database';
import { getAuth, GoogleAuthProvider, signInWithPopup, signOut, onAuthStateChanged } from 'firebase/auth';

const firebaseConfig = {
  apiKey: "AIzaSyA1TtZCiI_lDs-qiYY5raUAQFNdMFcRn3g",
  authDomain: "copy-and-paste-online.firebaseapp.com",
  databaseURL: "https://copy-and-paste-online-default-rtdb.firebaseio.com",
  projectId: "copy-and-paste-online",
  storageBucket: "copy-and-paste-online.firebasestorage.app",
  messagingSenderId: "338015118159",
  appId: "1:338015118159:web:359ace8d480271bc215b3a"
};

const app = initializeApp(firebaseConfig);
const db = getDatabase(app);

// 파일 메타데이터 업데이트 (저장 시 호출)
export function updateFileMeta(userId, filePath, { size, hash }) {
  const safeKey = filePath.replace(/\./g, '_dot_').replace(/\//g, '_slash_');
  const fileRef = ref(db, `mdflare/${userId}/files/${safeKey}`);
  return set(fileRef, {
    path: filePath,
    size,
    modified: Date.now(),
    hash
  });
}

// 파일 변경 리스너 (실시간 감지)
export function onFilesChanged(userId, callback) {
  const filesRef = ref(db, `mdflare/${userId}/files`);
  return onValue(filesRef, (snapshot) => {
    const data = snapshot.val();
    if (data) {
      callback(Object.values(data));
    }
  });
}

// 간단한 해시 생성
export function simpleHash(str) {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    const char = str.charCodeAt(i);
    hash = ((hash << 5) - hash) + char;
    hash |= 0;
  }
  return hash.toString(36);
}

// Auth
const auth = getAuth(app);
const googleProvider = new GoogleAuthProvider();

export function loginWithGoogle() {
  return signInWithPopup(auth, googleProvider);
}

export function logout() {
  return signOut(auth);
}

export function onAuthChange(callback) {
  return onAuthStateChanged(auth, callback);
}

export { db, auth };
