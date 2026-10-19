{{- define "torchio.labels" -}}
app.kubernetes.io/name: torchio
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version }}
{{- end }}

{{- define "torchio.selectorLabels" -}}
app.kubernetes.io/name: torchio
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "torchio.fullname" -}}
{{- if contains "torchio" .Release.Name }}{{ .Release.Name | trunc 63 | trimSuffix "-" }}{{ else }}{{ printf "%s-torchio" .Release.Name | trunc 63 | trimSuffix "-" }}{{ end }}
{{- end }}
