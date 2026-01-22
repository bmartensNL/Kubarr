# Kubarr - Automated Tool Installation Script
# Run this script as Administrator

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  Kubarr - Tool Installation Script" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Check if running as Administrator
$currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
$isAdmin = $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $isAdmin) {
    Write-Host "ERROR: This script must be run as Administrator!" -ForegroundColor Red
    Write-Host ""
    Write-Host "Right-click PowerShell and select 'Run as Administrator', then run this script again." -ForegroundColor Yellow
    Write-Host ""
    Pause
    Exit 1
}

Write-Host "Running as Administrator - Good!" -ForegroundColor Green
Write-Host ""

# Check if Chocolatey is installed
Write-Host "Checking for Chocolatey..." -ForegroundColor Cyan
try {
    $chocoVersion = choco --version 2>&1
    Write-Host "  Chocolatey is installed: $chocoVersion" -ForegroundColor Green
} catch {
    Write-Host "  Chocolatey not found. Installing..." -ForegroundColor Yellow
    Set-ExecutionPolicy Bypass -Scope Process -Force
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
    Invoke-Expression ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
    Write-Host "  Chocolatey installed!" -ForegroundColor Green
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Installing Required Tools..." -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Install Node.js LTS
Write-Host "[1/4] Installing Node.js LTS..." -ForegroundColor Cyan
try {
    $nodeVersion = node --version 2>&1
    Write-Host "  Node.js already installed: $nodeVersion" -ForegroundColor Yellow
} catch {
    choco install nodejs-lts -y
    Write-Host "  Node.js installed!" -ForegroundColor Green
}

Write-Host ""

# Install kind
Write-Host "[2/4] Installing kind (Kubernetes in Docker)..." -ForegroundColor Cyan
try {
    $kindVersion = kind --version 2>&1
    Write-Host "  kind already installed: $kindVersion" -ForegroundColor Yellow
} catch {
    choco install kind -y
    Write-Host "  kind installed!" -ForegroundColor Green
}

Write-Host ""

# Update kubectl
Write-Host "[3/4] Installing/Updating kubectl..." -ForegroundColor Cyan
try {
    choco uninstall kubernetes-cli -y 2>&1 | Out-Null
} catch {
    # Ignore errors if not installed
}
choco install kubernetes-cli -y
Write-Host "  kubectl installed!" -ForegroundColor Green

Write-Host ""

# Install Helm
Write-Host "[4/4] Installing Helm v3..." -ForegroundColor Cyan
try {
    $helmVersion = helm version --short 2>&1
    Write-Host "  Helm already installed: $helmVersion" -ForegroundColor Yellow
} catch {
    choco install kubernetes-helm -y
    Write-Host "  Helm installed!" -ForegroundColor Green
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Refreshing Environment..." -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
refreshenv

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Verification" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Verify installations
Write-Host "Checking installed versions:" -ForegroundColor Cyan
Write-Host ""

Write-Host "Node.js: " -NoNewline
try {
    $nodeVer = node --version 2>&1
    Write-Host "$nodeVer" -ForegroundColor Green
} catch {
    Write-Host "NOT FOUND" -ForegroundColor Red
}

Write-Host "npm: " -NoNewline
try {
    $npmVer = npm --version 2>&1
    Write-Host "$npmVer" -ForegroundColor Green
} catch {
    Write-Host "NOT FOUND" -ForegroundColor Red
}

Write-Host "kind: " -NoNewline
try {
    $kindVer = kind --version 2>&1
    Write-Host "$kindVer" -ForegroundColor Green
} catch {
    Write-Host "NOT FOUND" -ForegroundColor Red
}

Write-Host "kubectl: " -NoNewline
try {
    $kubectlVer = kubectl version --client --short 2>&1 | Select-String "Client Version"
    Write-Host "$kubectlVer" -ForegroundColor Green
} catch {
    Write-Host "NOT FOUND" -ForegroundColor Red
}

Write-Host "Helm: " -NoNewline
try {
    $helmVer = helm version --short 2>&1
    Write-Host "$helmVer" -ForegroundColor Green
} catch {
    Write-Host "NOT FOUND" -ForegroundColor Red
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host "Installation Complete!" -ForegroundColor Green
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "IMPORTANT: Please close this PowerShell window and open a NEW terminal" -ForegroundColor Yellow
Write-Host "to ensure all PATH changes take effect." -ForegroundColor Yellow
Write-Host ""
Write-Host "Next steps:" -ForegroundColor Cyan
Write-Host "  1. Open a NEW PowerShell or terminal window" -ForegroundColor White
Write-Host "  2. Navigate to: cd c:\Users\admin\Projects\Kubarr" -ForegroundColor White
Write-Host "  3. Run: poetry shell" -ForegroundColor White
Write-Host "  4. Follow the instructions in INSTALL_STATUS.md" -ForegroundColor White
Write-Host ""

Pause
