{{/*
Expand the name of the chart.
*/}}
{{- define "deluge.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name.
*/}}
{{- define "deluge.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Create chart name and version as used by the chart label.
*/}}
{{- define "deluge.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels
*/}}
{{- define "deluge.labels" -}}
helm.sh/chart: {{ include "deluge.chart" . }}
{{ include "deluge.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
managed-by: kubarr
category: download-client
{{- end }}

{{/*
Selector labels
*/}}
{{- define "deluge.selectorLabels" -}}
app.kubernetes.io/name: {{ include "deluge.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app: deluge
{{- end }}

{{/*
Create the name of the service account to use
*/}}
{{- define "deluge.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "deluge.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}
