# acme-distributor client for MikroTik RouterOS 7
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

:do {
  :local Res [ /tool/fetch url=($Server . "/certificate/" . $Domain) \
    http-header-field=({ ("x-credential: " . $Token) }) \
    check-certificate=$CheckCert output=user as-value ]
  :local Json [ :deserialize from=json value=($Res->"data") ]
  :local Cert [ :tostr ($Json->"cert") ]
  :local Ca [ :tostr ($Json->"ca") ]
  :local Key [ :tostr ($Json->"key") ]
  :local Expires [ :tostr ($Json->"expires_at") ]

  :if ([ :len $Cert ] = 0 or [ :len $Key ] = 0) do={
    :error "server returned an empty certificate"
  }

  :local Stamp ""
  :if ([ :len [ /file/find where name=$StampFile ] ] > 0) do={
    :set Stamp [ /file/get [ /file/find where name=$StampFile ] contents ]
  }

  :if ($Stamp = $Expires and [ :len [ /certificate/find where name~("^" . $CertName) ] ] > 0) do={
    :log debug ("acme-distributor: certificate for " . $Domain . " is up to date")
  } else={
    $WriteFile $PemFile $Cert
    $WriteFile $KeyFile $Key
    :if ([ :len $Ca ] > 0) do={ $WriteFile $CaFile $Ca }

    # free the name, but keep the old certificate until services are repointed
    /certificate/remove [ find where name~("^old-" . $CertName) ]
    :foreach C in=[ /certificate/find where name~("^" . $CertName) ] do={
      /certificate/set $C name=("old-" . [ /certificate/get $C name ])
    }

    /certificate/import file-name=$PemFile name=$CertName passphrase="" as-value
    /certificate/import file-name=$KeyFile passphrase="" as-value
    :if ([ :len $Ca ] > 0) do={
      /certificate/import file-name=$CaFile name=($CertName . "-ca") passphrase="" as-value
    }
    /file/remove [ find where name=$PemFile or name=$KeyFile or name=$CaFile ]

    :local Leaf [ /certificate/find where name=$CertName ]
    :if ([ :len $Leaf ] = 0) do={ :set Leaf [ /certificate/find where name=($CertName . "_0") ] }
    :if ([ :len $Leaf ] != 1) do={ :error "imported certificate not found" }
    :local LeafName [ /certificate/get ($Leaf->0) name ]

    # ponytail: only www-ssl/api-ssl are repointed, add hotspot/ovpn/ipsec to $Services if used
    :foreach S in=$Services do={
      /ip/service/set [ find where name=$S ] certificate=$LeafName
    }

    :do {
      /certificate/remove [ find where name~("^old-" . $CertName) ]
    } on-error={
      :log warning ("acme-distributor: old certificate for " . $Domain . " is still in use, kept")
    }

    /file/remove [ find where name=$StampFile ]
    /file/add name=$StampFile contents=$Expires
    :log info ("acme-distributor: installed " . $LeafName . " for " . $Domain . ", valid until " . $Expires)
  }
} on-error={
  :set AcmeDistData ""
  /file/remove [ find where name=$PemFile or name=$KeyFile or name=$CaFile ]
  :log error ("acme-distributor: updating certificate for " . $Domain . " failed")
}
