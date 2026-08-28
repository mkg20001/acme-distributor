# acme-distributor client for MikroTik RouterOS 7.13+
#
# Fetches a certificate daily and installs it as the router's TLS certificate.
# Only touches the router when the certificate actually changed (compared by
# expiry stamp), so daily runs are cheap no-ops in between renewals.
#
# Install (upload this file to the router, then):
#   /system/script/add name=acme-distributor policy=read,write,policy,test \
#     source=[/file/get [find where name="routeros.rsc"] contents]
#   /system/scheduler/add name=acme-distributor interval=1d start-time=03:00:00 \
#     policy=read,write,policy,test on-event="/system/script/run acme-distributor"
#
# The router needs working DNS and clock, and must trust the CA of $Server
# (import it with /certificate/import, or use plain http on a trusted network).

:local Server "https://acme.example.com:3444"
:local Token "your-token-here"
:local Domain "router.oliver-koss.at"
:local Services ({ "www-ssl"; "api-ssl" })
:local CheckCert "yes-without-crl"

:local CertName ("acme-" . $Domain)
:local PemFile ($CertName . ".pem")
:local CaFile ($CertName . ".ca.pem")
:local KeyFile ($CertName . ".key")
:local StampFile ($CertName . ".stamp")

# /file/add contents= is capped at 4095 bytes, so PEMs are written via :execute
:global AcmeDistData

:local WriteFile do={
  :global AcmeDistData
  :set AcmeDistData [ :tostr $2 ]
  /file/remove [ find where name=$1 ]
  :execute script={ :global AcmeDistData; :put $AcmeDistData } file=([ :tostr $1 ] . "\00")
  :for I from=1 to=20 do={
    :if ([ :len [ /file/find where name=$1 ] ] = 0) do={ :delay 500ms }
  }
  :delay 500ms
  :set AcmeDistData ""
  :if ([ :len [ /file/find where name=$1 ] ] = 0) do={ :error ("writing " . $1 . " timed out") }
}

:onerror Err {
  :local Res [ /tool/fetch url=($Server . "/certificate/" . $Domain) \
    http-header-field=({ ("x-credential: " . $Token) }) \
    check-certificate=$CheckCert output=user as-value ]
  :local Json [ :deserialize from=json value=($Res->"data") ]
  :local Cert [ :tostr ($Json->"cert") ]
  :local Ca [ :tostr ($Json->"ca") ]
  :local Key [ :tostr ($Json->"key") ]
  :local Expires [ :tostr ($Json->"expiresAt") ]

  :if ([ :len $Cert ] = 0 or [ :len $Key ] = 0 or [ :len $Expires ] = 0) do={
    :error "server returned an empty certificate"
  }

  :local Stamp ""
  :if ([ :len [ /file/find where name=$StampFile ] ] > 0) do={
    :set Stamp [ /file/get [ /file/find where name=$StampFile ] contents ]
  }

  :local Fresh false
  :if ($Stamp = $Expires and [ :len [ /certificate/find where name~("^" . $CertName) ] ] > 0) do={
    :log debug ("acme-distributor: certificate for " . $Domain . " is up to date")
  } else={
    $WriteFile $PemFile $Cert
    $WriteFile $KeyFile $Key
    :if ([ :len $Ca ] > 0) do={ $WriteFile $CaFile $Ca }

    # RouterOS refuses to import a certificate that is already in the store,
    # so the old entries go first: services keep a dangling reference for the
    # moment it takes to import and repoint them below
    /certificate/remove [ find where name~("^" . $CertName) ]

    /certificate/import file-name=$PemFile name=$CertName passphrase="" as-value
    /certificate/import file-name=$KeyFile passphrase="" as-value
    :if ([ :len $Ca ] > 0) do={
      /certificate/import file-name=$CaFile name=($CertName . "-ca") passphrase="" as-value
    }
    /file/remove [ find where name=$PemFile or name=$KeyFile or name=$CaFile ]

    :set Fresh true
    /file/remove [ find where name=$StampFile ]
    /file/add name=$StampFile contents=$Expires
  }

  :local Leaf [ /certificate/find where name=$CertName ]
  :if ([ :len $Leaf ] = 0) do={ :set Leaf [ /certificate/find where name=($CertName . "_0") ] }
  :if ([ :len $Leaf ] != 1) do={ :error "certificate not found after import" }
  :local LeafName [ /certificate/get ($Leaf->0) name ]

  :if ($Fresh = true) do={
    :log info ("acme-distributor: imported " . $LeafName . " for " . $Domain . ", expires at unix " . $Expires)
  }

  # enforce the service binding on every run, not just on renewal: the import
  # above leaves a dangling reference, and manual changes get corrected too
  # ponytail: only /ip/service entries, add hotspot/ovpn/ipsec profiles if used
  :foreach S in=$Services do={
    :local Svc [ /ip/service/find where name=$S ]
    :if ([ :len $Svc ] != 1) do={
      :log warning ("acme-distributor: no service named " . $S)
    } else={
      :if ([ /ip/service/get ($Svc->0) certificate ] != $LeafName) do={
        /ip/service/set ($Svc->0) certificate=$LeafName
        :log info ("acme-distributor: pointed " . $S . " at " . $LeafName)
      }
    }
  }
} do={
  :set AcmeDistData ""
  /file/remove [ find where name=$PemFile or name=$KeyFile or name=$CaFile ]
  :log error ("acme-distributor: updating certificate for " . $Domain . " failed: " . $Err)
}
