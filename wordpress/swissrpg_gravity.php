<?php
/**
 * Plugin Name:       SwissRPG Gravity Plugin
 * Description:       Adds Discord username validation to Gravity forms.
 * Version:           0.1
 * Requires at least: 5.2
 * Requires PHP:      7.2
 * Author:            Daniel Thul
 * License:           BSD 3-clause
 * License URI:       https://opensource.org/licenses/BSD-3-Clause
 */
namespace SwissRPG_Gravity_Plugin;

function validate_form($result, $value, $form, $field)
{
    if ($result['is_valid'] && $field->type == 'text' && trim(strtolower($field->adminLabel)) == 'discord') {
        // Check if the specified Discord user is a member of our Discord server
        $response = wp_remote_get(
            'https://bot.swissrpg.ch/api/check_discord_username',
            array(
                'timeout' => 3,
                'headers' => array(
                    // 'Api-Key' => /* TODO */,
                    'Discord-Username' => $value,
                ),
                'limit_response_size' => 1024,
            )
        );
        if (is_wp_error($response)) {
            return $result;
        }
        $http_code = wp_remote_retrieve_response_code($response);
        // HTTP response codes:
        // - 200: Discord username found
        // - 204: Discord username not found
        // - any other: request failed
        if ($http_code == 204) {
            // Discord username was not found.
            // Add a validation error
            $result['is_valid'] = false;
            $result['message'] = empty($field->errorMessage)
            ? 'Discord username is invalid or the associated Discord user is not a member of the SwissRPG Discord server.'
            : $field->errorMessage;
        }
    }
    return $result;
}

add_filter('gform_field_validation', __NAMESPACE__ . '\\validate_form', /*priority=*/10, /*accepted_args=*/4);