package auth

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"errors"
	"fmt"
	"runtime"
	"strings"

	"golang.org/x/crypto/argon2"
)

const (
	argonMemory uint32 = 64 * 1024
	argonTime   uint32 = 3
	argonKeyLen uint32 = 32
)

func HashPassword(password string) (string, error) {
	salt := make([]byte, 16)
	if _, err := rand.Read(salt); err != nil {
		return "", err
	}
	threads := uint8(runtime.NumCPU())
	if threads > 4 {
		threads = 4
	}
	if threads < 1 {
		threads = 1
	}
	hash := argon2.IDKey([]byte(password), salt, argonTime, argonMemory, threads, argonKeyLen)
	b64 := base64.RawStdEncoding
	return fmt.Sprintf("$argon2id$v=19$m=%d,t=%d,p=%d$%s$%s", argonMemory, argonTime, threads, b64.EncodeToString(salt), b64.EncodeToString(hash)), nil
}

func VerifyPassword(password, encoded string) (bool, error) {
	parts := strings.Split(encoded, "$")
	if len(parts) != 6 || parts[1] != "argon2id" || parts[2] != "v=19" {
		return false, errors.New("invalid password hash")
	}
	var memory, timeCost uint32
	var threads uint8
	if _, err := fmt.Sscanf(parts[3], "m=%d,t=%d,p=%d", &memory, &timeCost, &threads); err != nil {
		return false, errors.New("invalid password parameters")
	}
	if memory > 256*1024 || timeCost > 10 || threads > 16 || memory < 8*1024 || timeCost < 1 || threads < 1 {
		return false, errors.New("unsafe password parameters")
	}
	salt, err := base64.RawStdEncoding.DecodeString(parts[4])
	if err != nil {
		return false, errors.New("invalid salt")
	}
	want, err := base64.RawStdEncoding.DecodeString(parts[5])
	if err != nil {
		return false, errors.New("invalid hash")
	}
	got := argon2.IDKey([]byte(password), salt, timeCost, memory, threads, uint32(len(want)))
	return subtle.ConstantTimeCompare(got, want) == 1, nil
}
