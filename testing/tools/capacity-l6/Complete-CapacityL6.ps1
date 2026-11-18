#requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$RunRoot,
    [Parameter(Mandatory)][string[]]$Cases,
    [Parameter(Mandatory)][string]$Assessment,
    [Parameter(Mandatory)][string]$AssessmentSha256,
    [switch]$ConfirmIndependentReview
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
try {
    Import-Module (Join-Path $PSScriptRoot 'CapacityL6Review.psm1') -Force
    $result = Complete-CapacityL6Review @PSBoundParameters
    $result | ConvertTo-Json -Depth 8
    if ($result.acceptancePassed) { exit 0 }
    exit 2
} catch {
    # Never echo JSON, private capture contents, profile values, or raw errors.
    [ordered]@{ status = 'failed'; acceptancePassed = $false
        errorCategory = 'independent_review_validation' } | ConvertTo-Json -Compress
    exit 1
}
