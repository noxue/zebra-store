# Edge proxy: Caddy + the two built SPAs (storefront at /, admin at /admin/).
# Both Zebra instances (store, zs2) and all reseller subsites share these files:
# branding comes from /api/v1/public/config at runtime.
FROM caddy:2.10-alpine
COPY build/web/storefront /srv/storefront
COPY build/web/admin /srv/admin
