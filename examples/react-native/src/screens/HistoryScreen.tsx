import React from 'react'
import { View, Text, ScrollView } from 'react-native'
import s from '../styles'
import { SectionCard, SectionTitle } from '../components/common'

export const HistoryScreen = () => {
  return (
    <ScrollView style={s.container} contentContainerStyle={s.contentContainer}>
      <Text style={s.header}>History</Text>
      <SectionCard>
        <SectionTitle>Transaction History</SectionTitle>
        <Text style={s.italic}>
          Transaction history is not fully exposed directly via the simple SDK wrappers in this demo yet. 
          Check the main typescript fedimint core for advanced capabilities.
        </Text>
      </SectionCard>
    </ScrollView>
  )
}
