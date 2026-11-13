// Added to libtailscale's source for the phone's build (ADR-0016, [[EXP-20261113-1h4ztf]]).
//
// Since Android 11 an app may not bind a NETLINK_ROUTE socket, so Go's net.Interfaces fails
// and tsnet cannot start ("route ip+net: netlinkrib: permission denied"). Tailscale's own
// app hands netmon its interfaces from Java; a cgo build for Android is expected to do the
// same, and tailscale.com/feature/androidbin stays out of it. Bionic's getifaddrs still
// works for apps, so the interfaces come from there, and the interface the kernel routes
// outbound traffic through is reported as the default route, which netmon on Android
// otherwise learns only from the app.
//
// It also gives tsnet a place for its log state: an app has no HOME, systemd state
// directory or writable temp directory, so logpolicy.LogsDir panics ("no safe place found
// to store log state") unless TS_LOGS_DIR names one. Go copies the environment when the
// library loads, so the host sets it through tailscale_android_set_logs_dir.

//go:build android

package main

/*
#include <ifaddrs.h>
#include <net/if.h>
#include <netinet/in.h>
#include <sys/socket.h>
*/
import "C"

import (
	"net"
	"os"
	"unsafe"

	"tailscale.com/net/netmon"
)

func init() {
	netmon.RegisterInterfaceGetter(androidInterfaces)
}

//export tailscale_android_set_logs_dir
func tailscale_android_set_logs_dir(dir *C.char) C.int {
	if err := os.Setenv("TS_LOGS_DIR", C.GoString(dir)); err != nil {
		return -1
	}
	return 0
}

func androidInterfaces() ([]netmon.Interface, error) {
	var head *C.struct_ifaddrs
	if rc, err := C.getifaddrs(&head); rc != 0 {
		return nil, err
	}
	defer C.freeifaddrs(head)

	var out []netmon.Interface
	at := map[string]int{}
	for p := head; p != nil; p = p.ifa_next {
		name := C.GoString(p.ifa_name)
		k, ok := at[name]
		if !ok {
			k = len(out)
			at[name] = k
			out = append(out, netmon.Interface{
				Interface: &net.Interface{
					Index: int(C.if_nametoindex(p.ifa_name)),
					Name:  name,
					Flags: flags(uint(p.ifa_flags)),
				},
				AltAddrs: []net.Addr{},
			})
		}
		if a := ipNet(p.ifa_addr, p.ifa_netmask); a != nil {
			out[k].AltAddrs = append(out[k].AltAddrs, a)
		}
	}

	// The interface holding the source address of an outbound route. Connecting a UDP
	// socket sends nothing; it only makes the kernel pick a route.
	if c, err := net.Dial("udp4", "8.8.8.8:53"); err == nil {
		src := c.LocalAddr().(*net.UDPAddr).IP
		c.Close()
		for _, i := range out {
			for _, a := range i.AltAddrs {
				if n, ok := a.(*net.IPNet); ok && n.IP.Equal(src) {
					netmon.UpdateLastKnownDefaultRouteInterface(i.Name)
				}
			}
		}
	}
	return out, nil
}

func flags(f uint) net.Flags {
	var out net.Flags
	for _, m := range []struct {
		c uint
		g net.Flags
	}{
		{C.IFF_UP, net.FlagUp},
		{C.IFF_BROADCAST, net.FlagBroadcast},
		{C.IFF_LOOPBACK, net.FlagLoopback},
		{C.IFF_POINTOPOINT, net.FlagPointToPoint},
		{C.IFF_MULTICAST, net.FlagMulticast},
		{C.IFF_RUNNING, net.FlagRunning},
	} {
		if f&m.c != 0 {
			out |= m.g
		}
	}
	return out
}

func ipNet(addr, mask *C.struct_sockaddr) *net.IPNet {
	if addr == nil {
		return nil
	}
	switch addr.sa_family {
	case C.AF_INET:
		ip := C.GoBytes(unsafe.Pointer(&(*C.struct_sockaddr_in)(unsafe.Pointer(addr)).sin_addr), 4)
		m := net.CIDRMask(32, 32)
		if mask != nil {
			m = net.IPMask(C.GoBytes(unsafe.Pointer(&(*C.struct_sockaddr_in)(unsafe.Pointer(mask)).sin_addr), 4))
		}
		return &net.IPNet{IP: net.IP(ip), Mask: m}
	case C.AF_INET6:
		ip := C.GoBytes(unsafe.Pointer(&(*C.struct_sockaddr_in6)(unsafe.Pointer(addr)).sin6_addr), 16)
		m := net.CIDRMask(128, 128)
		if mask != nil {
			m = net.IPMask(C.GoBytes(unsafe.Pointer(&(*C.struct_sockaddr_in6)(unsafe.Pointer(mask)).sin6_addr), 16))
		}
		return &net.IPNet{IP: net.IP(ip), Mask: m}
	}
	return nil
}
