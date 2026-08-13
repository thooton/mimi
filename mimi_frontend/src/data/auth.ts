import { useEffect, useState } from 'react';
import type { ApiViewer } from './api';
import {
  changeEmail,
  changePassword,
  fetchViewer,
  loginUser,
  logoutUser,
  registerUser,
  startGuestUser,
} from './api';

const EVENT = 'mimi:auth';

interface AuthState {
  user: ApiViewer | null;
  ready: boolean;
}

let current: AuthState = { user: null, ready: false };
let inflight: Promise<void> | null = null;

function announce() {
  window.dispatchEvent(new CustomEvent(EVENT));
}

function restore() {
  if (inflight) return;
  inflight = fetchViewer()
    .then((user) => {
      current = { user, ready: true };
    })
    .catch(() => {
      current = { user: null, ready: true };
    })
    .finally(announce);
}

async function signIn(login: string, password: string) {
  const user = await loginUser(login, password);
  current = { user, ready: true };
  announce();
  return user;
}

async function signUp(username: string, email: string, password: string) {
  const user = await registerUser(username, email, password);
  current = { user, ready: true };
  announce();
  return user;
}

/* Begin as a guest: a real backend account with no credentials, so the course
   can start now and the sign-up can wait. Registering later claims the record
   rather than starting a new one (see registerUser), which is why the prompt
   after each lesson can honestly call itself "save your progress". */
async function startAsGuest() {
  const user = await startGuestUser();
  current = { user, ready: true };
  announce();
  return user;
}

/* The two account settings. A password leaves the viewer as it was, there is
   nothing about it here to go stale, but an address is on display in the
   settings page and comes back from the backend, so the store takes the
   answer rather than the string that was typed. */
async function setPassword(currentPassword: string, newPassword: string) {
  await changePassword(currentPassword, newPassword);
}

async function setEmail(password: string, email: string) {
  const user = await changeEmail(password, email);
  current = { user, ready: true };
  announce();
  return user;
}

async function signOut() {
  try {
    await logoutUser();
  } finally {
    current = { user: null, ready: true };
    announce();
  }
}

export function authenticatedUser(): ApiViewer | null {
  return current.user;
}

export function useAuth() {
  const [state, setState] = useState<AuthState>(current);

  useEffect(() => {
    const sync = () => setState(current);
    sync();
    restore();
    window.addEventListener(EVENT, sync);
    return () => window.removeEventListener(EVENT, sync);
  }, []);

  return { ...state, signIn, signUp, signOut, startAsGuest, setEmail, setPassword };
}
