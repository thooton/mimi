package auth

import (
	"bytes"
	"compress/gzip"
	_ "embed"
	"fmt"
	"io"
	"strings"
	"sync"
)

// commonPasswordsGz is the 100,000 most used passwords, one per line.
//
// It is the same list MediaWiki screens against — SecLists'
// 10_million_password_list_top_100000, by way of wikimedia/common-passwords —
// and it is here so that every consumer gets the same answer. The wiki checked
// it and this service did not, which meant a password the sign-up form refused
// was still accepted by calling /v1/register directly.
//
// Kept compressed: 764K of text becomes 367K in the repository and in the
// binary, for a few milliseconds of startup.
//
//go:embed commonpasswords.txt.gz
var commonPasswordsGz []byte

// commonPasswords is the list as a set, decompressed once.
//
// strings.Split hands back substrings of one allocation rather than 100,000
// separate ones, so what this costs is the text itself plus the map's own
// overhead.
var commonPasswords = sync.OnceValue(func() map[string]struct{} {
	reader, err := gzip.NewReader(bytes.NewReader(commonPasswordsGz))
	if err != nil {
		panic(fmt.Sprintf("common password list is unreadable: %v", err))
	}
	defer reader.Close()
	raw, err := io.ReadAll(reader)
	if err != nil {
		panic(fmt.Sprintf("common password list is unreadable: %v", err))
	}
	lines := strings.Split(string(raw), "\n")
	set := make(map[string]struct{}, len(lines))
	for _, line := range lines {
		if line != "" {
			set[line] = struct{}{}
		}
	}
	return set
})

// IsCommonPassword reports whether a password is one of the most used ones.
//
// The comparison is exact, which is what MediaWiki does, and the list carries
// its own capitalised variants — "password", "Password" and "PASSWORD" are all
// in it — so folding case here would reject more than the wiki does rather than
// the same set.
func IsCommonPassword(password string) bool {
	_, found := commonPasswords()[password]
	return found
}
