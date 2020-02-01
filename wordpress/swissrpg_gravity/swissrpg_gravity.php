<?php
/**
 * Plugin Name:       Gravity Forms SwissRPG Add-On
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
    global $option_name, $option_field_api_key, $option_field_invalid_username_message;
    if ($result['is_valid'] && $field->type == 'text' && trim(strtolower($field->adminLabel)) == 'discord' && !empty($value)) {
        $options = get_option($option_name);
        if ($options == false) {
            // No options set
            return $result;
        }
        $api_key = $options[$option_field_api_key];
        $invalid_username_message = $options[$option_field_invalid_username_message];
        if (!isset($api_key) || !isset($invalid_username_message)) {
            // Options not set
            return $result;
        }
        // Check if the specified Discord user is a member of our Discord server
        $response = wp_remote_get(
            'https://bottest.swissrpg.ch/api/check_discord_username',
            array(
                'timeout' => 3,
                'headers' => array(
                    'Api-Key' => $api_key,
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
            ? $invalid_username_message
            : $field->errorMessage;
        }
    }
    return $result;
}

add_filter('gform_field_validation', __NAMESPACE__ . '\\validate_form', /*priority=*/10, /*accepted_args=*/4);

/** Settings */

$option_group = 'swissrpg';
$option_name = $option_group . '_options';
$option_field_api_key = $option_group . '_field_api_key';
$option_field_invalid_username_message = $option_group . '_invalid_username_message';
$page = 'swissrpg';

function install()
{
    global $option_name, $option_field_invalid_username_message;
    // Add initial options if they are not set
    $options = get_option($option_name);
    $create_option = false;
    if ($options == false) {
        $options = array();
        $create_option = true;
    }

    if (empty($options[$option_field_invalid_username_message])) {
        $options[$option_field_invalid_username_message] = 'Discord username is invalid or the associated Discord user is not a member of the SwissRPG Discord server.';
    }

    if ($create_option) {
        add_option($option_name, $options);
    } else {
        update_option($option_name, $options);
    }
}
register_activation_hook(__FILE__, __NAMESPACE__ . '\\install');

/**
 * Custom option and settings
 */
function settings_init()
{
    global $option_group, $option_name, $option_field_api_key, $option_field_invalid_username_message, $page;
    // Register a new setting for "swissrpg" page
    register_setting($option_group, $option_name);

    $settings_section = $option_group . '_section';
    // register a new section in the "swissrpg" page
    add_settings_section(
        /*id*/$settings_section,
        __('Settings', 'wporg'),
        /*callback*/__NAMESPACE__ . '\\section_cb',
        /*page=*/$page
    );

    // register a new field in the "wporg_section_developers" section, inside the "wporg" page
    add_settings_field(
        /*id*/$option_field_api_key, // as of WP 4.6 this value is used only internally
        // use $args' label_for to populate the id inside the callback
        /*title*/__('Hyperion API Key', 'wporg'),
        /*callback*/__NAMESPACE__ . '\\field_api_key_cb',
        /*page*/$page,
        /*section*/$settings_section,
        /*args*/[
            'label_for' => $option_field_api_key,
            // 'class' => 'wporg_row',
            // 'wporg_custom_data' => 'custom',
        ]
    );

    // register a new field in the "wporg_section_developers" section, inside the "wporg" page
    add_settings_field(
        /*id*/$option_field_invalid_username_message, // as of WP 4.6 this value is used only internally
        // use $args' label_for to populate the id inside the callback
        /*title*/__('Invalid username message', 'wporg'),
        /*callback*/__NAMESPACE__ . '\\field_invalid_username_message_cb',
        /*page*/$page,
        /*section*/$settings_section,
        /*args*/[
            'label_for' => $option_field_invalid_username_message,
            // 'class' => 'wporg_row',
            // 'wporg_custom_data' => 'custom',
        ]
    );
}

/**
 * register our wporg_settings_init to the admin_init action hook
 */
add_action('admin_init', __NAMESPACE__ . '\\settings_init');

/**
 * Options menu
 */
function options_page()
{
    global $page;
    add_options_page(
        /*page_title*/'SwissRPG Plugin Settings',
        /*menu_title*/'SwissRPG Plugin',
        /*capability*/'manage_options',
        /*menu_slug*/$page,
        /*function*/__NAMESPACE__ . '\\options_page_cb'
    );
}

/**
 * register our wporg_options_page to the admin_menu action hook
 */
add_action('admin_menu', __NAMESPACE__ . '\\options_page');

// /**
//  * Add an additional link to the settings page in the plugin listing
//  */
// function swissrpg_plugin_page_settings_link($links)
// {
//     global $page;
//     $links[] = '<a href="' .
//     admin_url('options-general.php?page=' . $page) .
//     '">' . __('Settings') . '</a>';
//     return $links;
// }

// add_filter('plugin_action_links_' . plugin_basename(__FILE__), __NAMESPACE__ . '\\swissrpg_plugin_page_settings_link');

/**
 * custom option and settings:
 * callback functions
 */

// developers section cb

// section callbacks can accept an $args parameter, which is an array.
// $args have the following keys defined: title, id, callback.
// the values are defined at the add_settings_section() function.
function section_cb($args)
{
    // Nothing to output
}

// field cb

// field callbacks can accept an $args parameter, which is an array.
// $args is defined at the add_settings_field() function.
// wordpress has magic interaction with the following keys: label_for, class.
// the "label_for" key value is used for the "for" attribute of the <label>.
// the "class" key value is used for the "class" attribute of the <tr> containing the field.
// you can add custom key value pairs to be used inside your callbacks.
function field_api_key_cb($args)
{
    global $option_name;
    // get the value of the setting we've registered with register_setting()
    $options = get_option($option_name);
    $field_name = $args['label_for'];
    $field_value = $options[$field_name];
    // output the field
    ?>
<input id="<?php echo esc_attr($field_name); ?>" type="text"
    name="<?php echo esc_attr($option_name); ?>[<?php echo esc_attr($field_name); ?>]"
    value="<?php echo esc_attr($field_value); ?>">
<?php
}

function field_invalid_username_message_cb($args)
{
    global $option_name;
    // get the value of the setting we've registered with register_setting()
    $options = get_option($option_name);
    $field_name = $args['label_for'];
    $field_value = $options[$field_name];
    // output the field
    ?>
<textarea id="<?php echo esc_attr($field_name); ?>"
    name="<?php echo esc_attr($option_name); ?>[<?php echo esc_attr($field_name); ?>]"><?php echo esc_html($field_value); ?></textarea>
<?php
}

/**
 * top level menu:
 * callback functions
 */
function options_page_cb()
{
    global $option_group, $page;
    // check user capabilities
    if (!current_user_can('manage_options')) {
        return;
    }

    // add error/update messages

    // check if the user have submitted the settings
    // wordpress will add the "settings-updated" $_GET parameter to the url
    if (isset($_GET['settings-updated'])) {
        // add settings saved message with the class of "updated"
        add_settings_error('swissrpg_messages', 'wporg_message', __('Settings Saved', 'wporg'), 'updated');
    }

    // show error/update messages
    settings_errors('swissrpg_messages');
    ?>
<div class="wrap">
    <h1><?php echo esc_html(get_admin_page_title()); ?></h1>
    <form action="options.php" method="post">
        <?php
// output security fields for the registered setting "wporg"
    settings_fields($option_group);
    // output setting sections and their fields
    // (sections are registered for "wporg", each field is registered to a specific section)
    do_settings_sections($page);
    // output save settings button
    submit_button();
    ?>
    </form>
</div>
<?php
}