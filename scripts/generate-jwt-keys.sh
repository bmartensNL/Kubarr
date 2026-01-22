#!/bin/bash

# Generate JWT RSA key pair for Kubarr OAuth2 provider
# This script creates a private/public key pair for signing JWT tokens

set -e

# Output directory
OUTPUT_DIR="${1:-.}"
PRIVATE_KEY_FILE="${OUTPUT_DIR}/jwt-private.pem"
PUBLIC_KEY_FILE="${OUTPUT_DIR}/jwt-public.pem"

echo "Generating RSA key pair for JWT signing..."

# Generate private key (2048-bit RSA)
openssl genrsa -out "${PRIVATE_KEY_FILE}" 2048

# Extract public key
openssl rsa -in "${PRIVATE_KEY_FILE}" -pubout -out "${PUBLIC_KEY_FILE}"

echo "✓ Private key generated: ${PRIVATE_KEY_FILE}"
echo "✓ Public key generated: ${PUBLIC_KEY_FILE}"

# Create Kubernetes secret YAML
NAMESPACE="${KUBARR_NAMESPACE:-kubarr-system}"
SECRET_NAME="kubarr-jwt-keys"

echo ""
echo "Generating Kubernetes secret manifest..."

# Read key files and base64 encode
PRIVATE_KEY_B64=$(base64 < "${PRIVATE_KEY_FILE}" | tr -d '\n')
PUBLIC_KEY_B64=$(base64 < "${PUBLIC_KEY_FILE}" | tr -d '\n')

cat > "${OUTPUT_DIR}/jwt-keys-secret.yaml" <<EOF
apiVersion: v1
kind: Secret
metadata:
  name: ${SECRET_NAME}
  namespace: ${NAMESPACE}
type: Opaque
data:
  jwt-private.pem: ${PRIVATE_KEY_B64}
  jwt-public.pem: ${PUBLIC_KEY_B64}
EOF

echo "✓ Kubernetes secret manifest: ${OUTPUT_DIR}/jwt-keys-secret.yaml"
echo ""
echo "To apply the secret to your cluster, run:"
echo "  kubectl apply -f ${OUTPUT_DIR}/jwt-keys-secret.yaml"
echo ""
echo "IMPORTANT: Keep the private key secure and do not commit it to version control!"
