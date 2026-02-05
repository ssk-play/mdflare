import { initializeApp } from 'firebase/app';
import { getDatabase, ref, set, onValue } from 'firebase/database';
import { getAuth, GoogleAuthProvider, signInWithPopup, signOut, onAuthStateChanged } from 'firebase/auth';

const firebaseConfig = {
  apiKey: "AIzaSyBfif2L4Cz11AYTP_OEsJZQJrNWe06a6CY",
  authDomain: "markdownflare.firebaseapp.com",
  databaseURL: "https://markdownflare-default-rtdb.firebaseio.com",
  projectId: "markdownflare",
  storageBucket: "markdownflare.firebasestorage.app",
  messagingSenderId: "658985431247",
  appId: "1:658985431247:web:07966fe83f40b1a7de07ba",
  measurementId: "G-VNWSZDGXV8"
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

export { db, auth, googleProvider };
